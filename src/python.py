import random
from collections import defaultdict

def sample_next(window, window_to_probs, rng):
    nexts = window_to_probs.get(window)
    if nexts is None:
        return None
    roll = rng.random()
    cumulative = 0.0
    for ch, prob in nexts.items():
        cumulative += prob
        if roll < cumulative:
            return ch
    return None

def main():
    with open("data/enwik8.txt", "r") as f:
        content = f.read()
    counts = defaultdict(lambda: defaultdict(int))
    chars = list(content.strip())
    window_length = 5
    for i in range(len(chars) - window_length):
        window = tuple(chars[i:i + window_length])
        next_char = chars[i + window_length]
        counts[window][next_char] += 1
    window_to_probs = {}
    for window, nexts in counts.items():
        total = sum(nexts.values())
        probs = {ch: count / total for ch, count in nexts.items()}
        window_to_probs[window] = probs
    # generate text
    window = list("There")
    for c in window:
        print(c, end="")
    rng = random.Random()
    for _ in range(500):
        next_char = sample_next(tuple(window), window_to_probs, rng)
        if next_char is not None:
            print(next_char, end="")
            window.pop(0)
            window.append(next_char)
        else:
            break
    print("")

if __name__ == "__main__":
    main()
