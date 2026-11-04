"""Exports one country's towns from Overture's divisions for `woopwoop build-overture`. Needs the duckdb package.

Usage: overture_places.py <release> <country code> <out.tsv>
"""
import sys

import duckdb

RELEASE, COUNTRY, OUT = sys.argv[1:4]
c = duckdb.connect()
c.execute("INSTALL httpfs;LOAD httpfs;SET s3_region='us-west-2';SET s3_url_style='path';SET threads=8;SET preserve_insertion_order=false;")
source = f"s3://overturemaps-us-west-2/release/{RELEASE}/theme=divisions/type=division/*"
c.execute(f"""COPY (SELECT names.primary name, country, replace(coalesce(region,''),country||'-','') region, subtype,
  coalesce(population,0) population, round((bbox.ymin+bbox.ymax)/2,6) lat, round((bbox.xmin+bbox.xmax)/2,6) lon
  FROM read_parquet('{source}') WHERE subtype='locality' AND names.primary IS NOT NULL AND country='{COUNTRY}')
  TO '{OUT}' (FORMAT CSV, DELIMITER '\t', HEADER false)""")
