#!/usr/bin/env python3
"""Checks a pack against well-known places, whose positions don't come from the pack's data.

Usage: landmarks.py <woopwoop binary> <pack> <tsv of text, lat, lon, building|city>
Buildings count within 1 km, cities within 15 km.
"""
import math
import re
import subprocess
import sys

binary, pack, path = sys.argv[1:4]


def metres(a, b, c, d):
    r = math.radians
    x = math.sin(r(c - a) / 2) ** 2 + math.cos(r(a)) * math.cos(r(c)) * math.sin(r(d - b) / 2) ** 2
    return 12742000 * math.asin(math.sqrt(x))


tally = {}
for line in open(path):
    text, lat, lon, group = line.rstrip('\n').split('\t')
    out = subprocess.run([binary, 'search', pack, text], capture_output=True, text=True).stdout.splitlines()
    m = re.match(r'(\w+)\s+(-?[\d.]+), (-?[\d.]+)\s+(.*)', out[0]) if out else None
    limit = 1000 if group == 'building' else 15000
    t = tally.setdefault(group, [0, 0, 0])
    t[0] += 1
    if not m:
        print(f'  none      {text}: {out[:1]}')
        continue
    d = metres(float(lat), float(lon), float(m[2]), float(m[3]))
    ok = d <= limit
    t[1] += ok
    t[2] += m[1] == 'House'
    print(f"  {'ok ' if ok else 'BAD'} {m[1]:9s}{d:9.0f} m  {text}  ->  {m[4]}")
for g, (n, ok, house) in tally.items():
    print(f'{g}: {ok} of {n} in range, {house} house level')
