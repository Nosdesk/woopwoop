"""Exports named roads inside bounding boxes from Overture for `woopwoop build-overture`. Needs the duckdb package.

Usage: overture_roads.py <release> <out.tsv> <west,south,east,north> [more boxes]
"""
import sys

import duckdb

RELEASE, OUT = sys.argv[1:3]
c = duckdb.connect()
c.execute("INSTALL httpfs;LOAD httpfs;SET s3_region='us-west-2';SET s3_url_style='path';SET threads=8;SET preserve_insertion_order=false;SET memory_limit='10GB';")
source = f"s3://overturemaps-us-west-2/release/{RELEASE}/theme=transportation/type=segment/*"
boxes = []
for box in sys.argv[3:]:
    w, s, e, n = (float(v) for v in box.split(','))
    boxes.append(f"(bbox.xmin BETWEEN {w} AND {e} AND bbox.ymin BETWEEN {s} AND {n})")
c.execute(f"""COPY (SELECT replace(replace(names.primary,chr(9),' '),chr(10),' ') name,
  round((bbox.ymin+bbox.ymax)/2,6) lat, round((bbox.xmin+bbox.xmax)/2,6) lon
  FROM read_parquet('{source}') WHERE subtype='road' AND names.primary IS NOT NULL
  AND class IN ('motorway','trunk','primary','secondary','tertiary','residential','unclassified','living_street')
  AND ({' OR '.join(boxes)})) TO '{OUT}' (FORMAT CSV, DELIMITER '\t', HEADER false)""")
