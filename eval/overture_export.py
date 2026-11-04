"""Exports one country from Overture's addresses for `woopwoop build-tsv`. Needs the duckdb package.

Usage: overture_export.py <release> <country code> <out.tsv>
"""
import sys

import duckdb

RELEASE, COUNTRY, OUT = sys.argv[1:4]
c = duckdb.connect()
c.execute("INSTALL httpfs;LOAD httpfs;SET s3_region='us-west-2';SET s3_url_style='path';SET threads=8;SET preserve_insertion_order=false;")
P = f"s3://overturemaps-us-west-2/release/{RELEASE}/theme=addresses/type=address/*"
c.execute(f"""COPY (SELECT replace(replace(number,chr(9),' '),chr(10),' ') number, replace(replace(street,chr(9),' '),chr(10),' ') street,
  coalesce(address_levels[2].value, postal_city, '') city, coalesce(postcode,'') postcode, coalesce(address_levels[1].value,'') region,
  round(bbox.ymin,7) lat, round(bbox.xmin,7) lon, coalesce(sources[1].dataset,'') dataset
  FROM read_parquet('{P}') WHERE country='{COUNTRY}' AND number IS NOT NULL AND street IS NOT NULL)
  TO '{OUT}' (FORMAT CSV, DELIMITER '\t', HEADER false)""")
