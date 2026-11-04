#!/usr/bin/env python3
"""Converts Ordnance Survey Code-Point Open into postcode rows for `woopwoop build-overture`.

Usage: codepoint.py <folder of Code-Point CSV files> <out.tsv>. Needs the pyproj package.
"""
import csv
import glob
import sys

from pyproj import Transformer

folder, path = sys.argv[1:3]
to_degrees = Transformer.from_crs(27700, 4326, always_xy=True)
with open(path, 'w') as out:
    for name in glob.glob(f'{folder}/*.csv'):
        for row in csv.reader(open(name)):
            east, north = int(row[2]), int(row[3])
            if east and north:
                lon, lat = to_degrees.transform(east, north)
                out.write(f'{row[0]}\tGB\t\tpostcode\t0\t{lat:.6f}\t{lon:.6f}\n')
