// Line-by-line C++ translation of the Rust markov-chain text generator.
// Standard library only (no external deps).
//
// Build:
//   c++ -std=c++17 -O2 -o markov markov.cpp
//
// Run:
//   ./markov     (expects data/shakespeare_full.txt next to the binary)
//
// UTF-8 handling: Rust's `chars()` yields Unicode scalar values, not bytes.
// We decode UTF-8 into uint32_t codepoints up front so the algorithm sees the
// same "characters" Rust does. The window key is std::array<uint32_t, 5>,
// which works directly as a key in std::unordered_map once we give it a hash.

#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <optional>
#include <random>
#include <sstream>
#include <string>
#include <unordered_map>
#include <vector>

static constexpr int WINDOW_LENGTH = 5;

using Window = std::array<uint32_t, WINDOW_LENGTH>;

// Hash for std::array<uint32_t, N> — std::unordered_map needs this.
struct WindowHash {
    std::size_t operator()(const Window& w) const noexcept {
        // FNV-1a 64-bit over the 20 bytes of the window. Cheap and good enough.
        std::uint64_t h = 1469598103934665603ull;
        for (uint32_t cp : w) {
            for (int i = 0; i < 4; i++) {
                h ^= (cp >> (i * 8)) & 0xFF;
                h *= 1099511628211ull;
            }
        }
        return static_cast<std::size_t>(h);
    }
};

// ---- UTF-8 decode --------------------------------------------------------

// Decode one codepoint at s (length len). Returns bytes consumed; writes
// codepoint to *out. On invalid input writes U+FFFD and returns 1 (resync).
static std::size_t utf8_decode(const unsigned char* s, std::size_t len,
                               uint32_t* out) {
    if (len == 0) { *out = 0xFFFD; return 0; }
    unsigned char b0 = s[0];
    if (b0 < 0x80) { *out = b0; return 1; }

    int extra;
    uint32_t cp;
    if      ((b0 & 0xE0) == 0xC0) { extra = 1; cp = b0 & 0x1F; }
    else if ((b0 & 0xF0) == 0xE0) { extra = 2; cp = b0 & 0x0F; }
    else if ((b0 & 0xF8) == 0xF0) { extra = 3; cp = b0 & 0x07; }
    else { *out = 0xFFFD; return 1; }

    if (static_cast<std::size_t>(extra) + 1 > len) { *out = 0xFFFD; return 1; }
    for (int i = 1; i <= extra; i++) {
        unsigned char b = s[i];
        if ((b & 0xC0) != 0x80) { *out = 0xFFFD; return 1; }
        cp = (cp << 6) | (b & 0x3F);
    }
    if (cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) {
        *out = 0xFFFD; return 1;
    }
    *out = cp;
    return static_cast<std::size_t>(extra + 1);
}

static void utf8_putcp(uint32_t cp) {
    unsigned char buf[4];
    int n;
    if (cp < 0x80) { buf[0] = static_cast<unsigned char>(cp); n = 1; }
    else if (cp < 0x800) {
        buf[0] = static_cast<unsigned char>(0xC0 | (cp >> 6));
        buf[1] = static_cast<unsigned char>(0x80 | (cp & 0x3F));
        n = 2;
    } else if (cp < 0x10000) {
        buf[0] = static_cast<unsigned char>(0xE0 | (cp >> 12));
        buf[1] = static_cast<unsigned char>(0x80 | ((cp >> 6) & 0x3F));
        buf[2] = static_cast<unsigned char>(0x80 | (cp & 0x3F));
        n = 3;
    } else {
        buf[0] = static_cast<unsigned char>(0xF0 | (cp >> 18));
        buf[1] = static_cast<unsigned char>(0x80 | ((cp >> 12) & 0x3F));
        buf[2] = static_cast<unsigned char>(0x80 | ((cp >> 6) & 0x3F));
        buf[3] = static_cast<unsigned char>(0x80 | (cp & 0x3F));
        n = 4;
    }
    std::fwrite(buf, 1, static_cast<std::size_t>(n), stdout);
}

// ---- algorithm -----------------------------------------------------------

// fn sample_next(...) -> Option<char>
static std::optional<uint32_t> sample_next(
    const Window& window,
    const std::unordered_map<Window,
                             std::unordered_map<uint32_t, double>,
                             WindowHash>& window_to_probs,
    std::mt19937_64& rng)
{
    auto it = window_to_probs.find(window);
    if (it == window_to_probs.end()) return std::nullopt;
    const auto& nexts = it->second;

    std::uniform_real_distribution<double> dist(0.0, 1.0);
    double roll = dist(rng);
    double cumulative = 0.0;
    for (const auto& [ch, prob] : nexts) {
        cumulative += prob;
        if (roll < cumulative) {
            return ch;
        }
    }
    return std::nullopt;
}

int main() {
    // let content: String = fs::read_to_string("data/shakespeare_full.txt")?;
    std::ifstream f("data/enwik8.txt", std::ios::binary);
    if (!f) {
        std::fprintf(stderr, "failed to open data/shakespeare_full.txt\n");
        return 1;
    }
    std::ostringstream ss;
    ss << f.rdbuf();
    std::string raw = ss.str();

    // let chars: Vec<char> = content.trim().chars().collect();
    std::vector<uint32_t> chars;
    chars.reserve(raw.size());
    {
        const unsigned char* p = reinterpret_cast<const unsigned char*>(raw.data());
        std::size_t remaining = raw.size();
        while (remaining > 0) {
            uint32_t cp;
            std::size_t consumed = utf8_decode(p, remaining, &cp);
            if (consumed == 0) break;
            chars.push_back(cp);
            p += consumed;
            remaining -= consumed;
        }
    }

    // trim (ASCII whitespace)
    auto is_ws = [](uint32_t c) {
        return c == ' ' || c == '\t' || c == '\n' || c == '\r' ||
               c == 0x0B || c == 0x0C;
    };
    std::size_t start = 0, end = chars.size();
    while (start < end && is_ws(chars[start])) start++;
    while (end > start && is_ws(chars[end - 1])) end--;

    // let mut counts: HashMap<Box<[char]>, HashMap<char, u32>> = HashMap::new();
    std::unordered_map<Window,
                       std::unordered_map<uint32_t, uint32_t>,
                       WindowHash> counts;

    // for w in chars.windows(window_length + 1) { ... }
    if (end - start >= static_cast<std::size_t>(WINDOW_LENGTH) + 1) {
        for (std::size_t i = start; i + WINDOW_LENGTH < end; i++) {
            Window window;
            for (int j = 0; j < WINDOW_LENGTH; j++) window[j] = chars[i + j];
            uint32_t next_char = chars[i + WINDOW_LENGTH];

            // *counts.entry(window).or_default().entry(next_char).or_insert(0) += 1;
            counts[window][next_char] += 1;
        }
    }

    // let window_to_probs = counts.into_iter().map(|(w, nexts)| { ... }).collect();
    std::unordered_map<Window,
                       std::unordered_map<uint32_t, double>,
                       WindowHash> window_to_probs;
    window_to_probs.reserve(counts.size());
    for (auto& [window, nexts] : counts) {
        // let total: u32 = nexts.values().sum();
        std::uint64_t total = 0;
        for (const auto& [ch, count] : nexts) total += count;

        std::unordered_map<uint32_t, double> probs;
        probs.reserve(nexts.size());
        for (const auto& [ch, count] : nexts) {
            probs.emplace(ch,
                static_cast<double>(count) / static_cast<double>(total));
        }
        window_to_probs.emplace(window, std::move(probs));
    }
    counts.clear();

    // generate text
    // let mut window: Vec<char> = "There".chars().collect();
    Window window = { 'T', 'h', 'e', 'r', 'e' };

    // for c in &window { print!("{c}"); }
    for (uint32_t c : window) utf8_putcp(c);

    // let mut rng = rand::rng();
    std::random_device rd;
    std::mt19937_64 rng(rd());

    // for _ in 0..500 { ... }
    for (int step = 0; step < 500; step++) {
        auto next = sample_next(window, window_to_probs, rng);
        if (!next.has_value()) break;

        utf8_putcp(*next);
        // window.rotate_left(1); *window.last_mut().unwrap() = next;
        for (int i = 0; i < WINDOW_LENGTH - 1; i++) {
            window[i] = window[i + 1];
        }
        window[WINDOW_LENGTH - 1] = *next;
    }

    // println!("");
    std::putchar('\n');
    return 0;
}
