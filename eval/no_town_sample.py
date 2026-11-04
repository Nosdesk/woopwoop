"""Samples addresses with no town in their data, and roads no address names, as a person would type them.
A sample drawn from addresses that name a town can't see how these do. The town typed is the nearest one, so
it favours a pack built with towns. Needs the duckdb package.

Usage: no_town_sample.py <country code> <addresses.tsv> <places.tsv> <roads.tsv> <out prefix> [seed]
"""
import csv,math,random,sys
import duckdb
cc,A,P,R,out=sys.argv[1:6]
seed=int(sys.argv[6]) if len(sys.argv)>6 else 42
towns={}
for r in csv.reader(open(P),delimiter='\t'):
    if len(r)<7 or r[3]!='locality': continue
    try:la,lo=float(r[5]),float(r[6])
    except ValueError: continue
    towns.setdefault((int(la*5),int(lo*5)),[]).append((la,lo,r[0],r[2]))
def near(la,lo):
    b=None
    for dy in range(-3,4):
        for dx in range(-3,4):
            for t in towns.get((int(la*5)+dy,int(lo*5)+dx),()):
                d=(t[0]-la)**2+((t[1]-lo)*math.cos(math.radians(la)))**2
                if b is None or d<b[0]:b=(d,t)
    return b[1] if b else None
c=duckdb.connect();c.execute("SET threads=8;SET memory_limit='10GB';")
cols="columns={'number':'VARCHAR','street':'VARCHAR','city':'VARCHAR','postcode':'VARCHAR','region':'VARCHAR','lat':'DOUBLE','lon':'DOUBLE','dataset':'VARCHAR'}"
n=len(open(A).readline().split('\t'))
if n==7: cols=cols.replace(",'dataset':'VARCHAR'","")
q=f"read_csv('{A}',delim='\t',header=false,quote='\"',ignore_errors=true,null_padding=true,{cols})"
rows=c.execute(f"SELECT * FROM (SELECT number,street,postcode,region,lat,lon FROM {q} WHERE (city IS NULL OR city='') AND street<>'') USING SAMPLE 8000 ROWS (reservoir, {seed})").fetchall()
rq=f"read_csv('{R}',delim='\t',header=false,quote='\"',ignore_errors=true,null_padding=true,columns={{'name':'VARCHAR','lat':'DOUBLE','lon':'DOUBLE'}})"
roads=c.execute(f"""WITH s AS (SELECT DISTINCT lower(street) n FROM {q})
 SELECT * FROM (SELECT name,lat,lon FROM {rq} WHERE lower(name) NOT IN (SELECT n FROM s)) USING SAMPLE 4000 ROWS (reservoir, {seed})""").fetchall()
L=[]
for num,st,pc,reg,la,lo in rows:
    t=near(la,lo)
    if not t: continue
    reg=reg or t[3]
    if pc: L.append((f"{num} {st}, {t[2]}, {reg} {pc}",la,lo,'town-and-postcode'))
    L.append((f"{num} {st}, {t[2]}, {reg}",la,lo,'town-only'))
    if pc: L.append((f"{num} {st}, {reg} {pc}",la,lo,'postcode-only'))
for nm,la,lo in roads:
    t=near(la,lo)
    if t: L.append((f"{nm}, {t[2]}, {t[3]}",la,lo,'road-only'))
random.seed(seed);random.shuffle(L);h=len(L)//2
for name,part in (('dev',L[:h]),('held',L[h:])):
    with open(f'{out}-{name}.tsv','w') as f:
        for x in part:f.write(f"{x[0]}\t{x[1]}\t{x[2]}\t{x[3]}\n")
print(cc,len(rows),len(roads),len(L))
