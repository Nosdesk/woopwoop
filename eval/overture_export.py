"""Exports one country from Overture's addresses for `woopwoop build-tsv`. Needs the duckdb package."""
import sys
import time

import duckdb

RELEASE, COUNTRY, OUT = sys.argv[1:4]
c=duckdb.connect();c.execute("INSTALL httpfs;LOAD httpfs;SET s3_region='us-west-2';SET s3_url_style='path';SET threads=8;SET preserve_insertion_order=false;")
P=f"s3://overturemaps-us-west-2/release/{RELEASE}/theme=addresses/type=address/*"
t=time.time()
c.execute(f"""COPY (SELECT replace(replace(number,chr(9),' '),chr(10),' ') number, replace(replace(street,chr(9),' '),chr(10),' ') street,
  coalesce(address_levels[2].value, postal_city, '') city, coalesce(postcode,'') postcode, coalesce(address_levels[1].value,'') region,
  round(bbox.ymin,7) lat, round(bbox.xmin,7) lon
  FROM read_parquet('{P}') WHERE country='{COUNTRY}' AND number IS NOT NULL AND street IS NOT NULL)
  TO '{OUT}' (FORMAT CSV, DELIMITER '\t', HEADER false)""")
print('exported',round(time.time()-t),'s')
