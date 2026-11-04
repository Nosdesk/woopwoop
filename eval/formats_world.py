#!/usr/bin/env python3
"""Address formats for `woopwoop eval` in Canada, New Zealand and the UK.

Usage: formats_world.py <sample.tsv of number, street, town, postcode, region, lat, lon> <CA|NZ|GB> [seed]
"""
import random
import sys

ABBR = {'Street': 'St', 'Road': 'Rd', 'Avenue': 'Ave', 'Drive': 'Dr', 'Court': 'Ct', 'Place': 'Pl', 'Lane': 'Ln', 'Boulevard': 'Blvd', 'Crescent': 'Cres', 'Terrace': 'Tce'}
country = sys.argv[2]
random.seed(int(sys.argv[3]) if len(sys.argv) > 3 else 5)


def typo(street):
    words = street.split()
    i = max(range(len(words)), key=lambda k: len(words[k]))
    w = list(words[i])
    if len(w) >= 4:
        j = random.randrange(1, len(w) - 1)
        w[j], w[j + 1] = w[j + 1], w[j]
    words[i] = ''.join(w)
    return ' '.join(words)


def spaced(code):
    code = code.replace(' ', '').upper()
    return f'{code[:-3]} {code[-3:]}' if len(code) > 3 else code


for line in open(sys.argv[1]):
    n, s, town, code, region, lat, lon = line.rstrip('\n').split('\t')
    s, town = s.title(), town.title()
    a = ' '.join(ABBR.get(w, w) for w in s.split())
    u = random.randint(1, 40)
    code = spaced(code) if country in ('CA', 'GB') else code
    tail = {'CA': f'{town}, {region} {code}', 'GB': f'{town} {code}', 'NZ': town}[country]
    short = {'CA': f'{town}, {region}', 'GB': town, 'NZ': town}[country]
    full = {'CA': f'{town}, {region}, Canada', 'GB': f'{town}, United Kingdom', 'NZ': f'{town}, New Zealand'}[country]
    formats = [('plain', f'{n} {s}, {tail}'), ('abbreviated', f'{n} {a}, {tail}'), ('lowercase', f'{n} {a} {tail}'.lower()),
               ('unit', f'Unit {u}, {n} {a}, {tail}'), ('no-postcode', f'{n} {s}, {short}'), ('with-country', f'{n} {s}, {full}'),
               ('typo-in-street', f'{n} {typo(s)}, {tail}')]
    if country == 'NZ':
        formats.append(('slash-unit', f'{u}/{n} {a}, {short}'))
    if country == 'CA':
        formats.append(('unit-first', f'{u}-{n} {a}, {tail}'))
    if country == 'GB':
        formats.append(('flat', f'Flat {u}, {n} {s}, {tail}'))
    for name, text in formats:
        print(f'{" ".join(text.split())}\t{lat}\t{lon}\t{name}')
