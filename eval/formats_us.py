#!/usr/bin/env python3
"""American address formats for `woopwoop eval`. Input: a TSV of number, street, city, zip, state, lat, lon."""
import random
import sys

ABBR = {'Street': 'St', 'Road': 'Rd', 'Avenue': 'Ave', 'Drive': 'Dr', 'Court': 'Ct', 'Place': 'Pl', 'Lane': 'Ln', 'Boulevard': 'Blvd',
        'Highway': 'Hwy', 'Circle': 'Cir', 'Parkway': 'Pkwy', 'Terrace': 'Ter', 'Trail': 'Trl', 'North': 'N', 'South': 'S', 'East': 'E', 'West': 'W'}

def typo(street):
    words = street.split()
    longest = max(range(len(words)), key=lambda i: len(words[i]))
    w = list(words[longest])
    if len(w) >= 4:
        i = random.randrange(1, len(w) - 1)
        w[i], w[i + 1] = w[i + 1], w[i]
    words[longest] = ''.join(w)
    return ' '.join(words)

random.seed(int(sys.argv[2]) if len(sys.argv) > 2 else 5)
for line in open(sys.argv[1]):
    n, s, city, z, st, lat, lon = line.rstrip('\n').split('\t')
    s = s.title()
    a = ' '.join(ABBR.get(w, w) for w in s.split())
    c = city.title()
    u = random.randint(1, 400)
    for name, text in [('plain', f'{n} {s}, {c}, {st} {z}'), ('abbreviated', f'{n} {a}, {c}, {st} {z}'), ('lowercase', f'{n} {a} {c} {st} {z}'.lower()),
                       ('apartment', f'{n} {a} Apt {u}, {c}, {st} {z}'), ('suite', f'{n} {s}, Suite {u}, {c}, {st}'), ('hash-unit', f'{n} {a} #{u}, {c} {st}'),
                       ('business-first', f'Acme Plumbing LLC, {n} {a}, {c}, {st} {z}'), ('no-zip', f'{n} {a}, {c}, {st}'), ('zip-only', f'{n} {a}, {z}'),
                       ('typo-in-street', f'{n} {typo(s)}, {c}, {st} {z}')]:
        print(f'{" ".join(text.split())}\t{lat}\t{lon}\t{name}')
