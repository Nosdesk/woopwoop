#!/usr/bin/env python3
"""Writes the address formats the evaluation scores, from a sample of real addresses.

Input: a TSV of number, street, locality, postcode, state, lat, lon.
Output, to stdout: text, lat, lon, format, for `woopwoop eval`.
"""
import random
import re
import sys

ABBR = {'STREET': 'St', 'ROAD': 'Rd', 'AVENUE': 'Ave', 'DRIVE': 'Dr', 'COURT': 'Ct', 'PLACE': 'Pl', 'CRESCENT': 'Cres',
        'LANE': 'Ln', 'TERRACE': 'Tce', 'CLOSE': 'Cl', 'PARADE': 'Pde', 'CIRCUIT': 'Cct', 'BOULEVARD': 'Bvd', 'HIGHWAY': 'Hwy'}
FULL = {'VIC': 'Victoria', 'NSW': 'New South Wales', 'QLD': 'Queensland', 'SA': 'South Australia', 'WA': 'Western Australia',
        'TAS': 'Tasmania', 'ACT': 'Australian Capital Territory', 'NT': 'Northern Territory', 'OT': ''}


def typo(street):
    chars = list(street)
    first = street.split()[0]
    if len(first) >= 4:
        i = random.randrange(1, len(first) - 1)
        chars[i], chars[i + 1] = chars[i + 1], chars[i]
    return ''.join(chars)


def formats(n, s, a, l, p, st, fs, u):
    top = int(re.match(r'\d+', n).group()) + 2
    return [
        ('plain', f'{n} {s}, {l} {st} {p}'),
        ('lowercase-abbreviated', f'{n} {a} {l} {p}'.lower()),
        ('unit-slash', f'{u}/{n} {a}, {l}'),
        ('unit-word', f'Unit {u}, {n} {a} {l} {st}'),
        ('level', f'Level {u}, {n} {s}, {l} {st} {p}'),
        ('number-range', f'{n}-{top} {a}, {l} {p}'),
        ('shop', f'Shop {u} {n} {a} {l}'),
        ('business-first', f'Acme Plumbing Pty Ltd, {n} {a}, {l} {st} {p}'),
        ('state-in-full', f'{n} {s} {l} {fs} Australia'),
        ('no-postcode', f'{n} {s}, {l}'),
        ('wrong-suburb', f'{n} {a}, Nowhereville {p}'),
        ('typo-in-street', f'{n} {typo(s)}, {l} {st} {p}'),
    ]


random.seed(int(sys.argv[2]) if len(sys.argv) > 2 else 5)
for line in open(sys.argv[1]):
    c = line.rstrip('\n').split('\t')
    n, street, locality, postcode, state, lat, lon = c[:7]
    words = street.split()
    a = ' '.join(ABBR.get(w, w) for w in words).title()
    for name, text in formats(n, street.title(), a, locality.title(), postcode, state, FULL.get(state, ''), random.randint(1, 40)):
        print(f'{text}\t{lat}\t{lon}\t{name}')
