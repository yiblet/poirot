"""Source-derived fixture builder; never imports or queries ROI."""
import json, math, re, datetime, decimal, collections, random
from pathlib import Path

ROOT=Path(__file__).parent
BASE='https://support.microsoft.com/en-us/excel/'
def url(n):
    return BASE+'functions/'+n.lower()+'-function'
S={'operators':BASE+'using-calculation-operators-in-excel-formulas',
   'intersection':BASE+'implicit-intersection-operator',
   'compat':'https://support.microsoft.com/en-gb/excel/compatibility-versions',
   'lazy':BASE+'detect-formula-errors-in-excel',
   'sum':BASE+'use-the-sum-function-to-sum-numbers-in-a-range',
   'is':BASE+'functions/is-functions',
   'find':'https://support.microsoft.com/en-gb/office/find-function-c7912941-af2a-4bdf-a553-d0d89b0a0628',
   'countif':BASE+'count-numbers-or-dates-based-on-a-condition-in-excel',
   'leap':'https://learn.microsoft.com/en-US/office/troubleshoot/excel/wrongly-assumes-1900-is-leap-year',
   'round':BASE+'round-a-number'}
groups=collections.defaultdict(list)
def v(x):
    if isinstance(x,dict): return x
    if x is None: return {'type':'blank','value':''}
    if isinstance(x,bool): return {'type':'boolean','value':x}
    if isinstance(x,(int,float)): return {'type':'number','value':x}
    if isinstance(x,list): return {'type':'utf16','value':x}
    return {'type':'error' if x.startswith('#') else 'text','value':x}
def cell(addr,x=None,formula=None,sheet=0):
    return dict(sheet=sheet,address=addr,**({'formula':formula} if formula is not None else {'value':v(x)}))
def add(cat,name,formula,expected,cells=(),sources=(),compat='Excel legacy / Compatibility Version 1',target='Z1',sheets=None,rationale=''):
    groups[cat].append(dict(name=name,category=cat,rationale=rationale,source_urls=list(sources),compatibility=compat,sheets=sheets or ['Sheet1'],cells=list(cells)+[cell(target,formula=formula)],target={'sheet':0,'address':target},expected=v(expected),**({'tolerance':1e-10} if isinstance(expected,float) else {})))

# Each sign, divisor, and rounding mode is a distinct numerical contract.
for n in [-10.5,-7,-1.5,-0.5,0,0.5,1.5,7,10.5]:
    for d in [-4,-2,0,2,4]:
        add('math','mod_'+str(n)+'_'+str(d),'=MOD(A1,B1)','#DIV/0!' if d==0 else n-d*math.floor(n/d),[cell('A1',n),cell('B1',d)],[url('mod')],rationale=f'Signed remainder for numerator {n} and divisor {d}; zero divisor must be an Excel error.')
for n in [-125.5,-15,-2.5,-0.5,0,0.5,2.5,15,125.5]:
    for digits in [-2,-1,0,1]:
        for fn,mode in [('ROUND',decimal.ROUND_HALF_UP),('ROUNDUP',decimal.ROUND_UP),('ROUNDDOWN',decimal.ROUND_DOWN),('TRUNC',decimal.ROUND_DOWN)]:
            out=float(decimal.Decimal(str(n)).quantize(decimal.Decimal(1).scaleb(-digits),rounding=mode))
            add('math',f'{fn}_{n}_{digits}',f'={fn}(A1,B1)',out,[cell('A1',n),cell('B1',digits)],[S['round'],url('trunc') if fn=='TRUNC' else url('round')],rationale=f'{fn} at {digits} decimal places distinguishes midpoint, sign, and left-of-decimal rounding for {n}.')
for n in [-3.99,-3,-0.01,0,0.01,3,3.99]:
    add('math',f'int_{n}','=INT(A1)',math.floor(n),[cell('A1',n)],[url('int')],rationale='Flooring a negative fractional number differs from truncation toward zero.')

# Aggregates must distinguish numeric cells from text/logicals and empty cells.
layouts=[[-4,0,8],[-4,None,8],[-4,'0',8],[-4,False,8],[-4,True,8],[-4,'x',8],[None,None,None],['2','3','4'],[True,False,True],[0,0,0],[1,-1,None],[2,3,4],['',None,0],[False,'',None],[1.5,-2.5,0]]
for ix,values in enumerate(layouts):
    nums=[x for x in values if isinstance(x,(int,float)) and not isinstance(x,bool)]
    for fn in ['SUM','AVERAGE','COUNT','COUNTA','COUNTBLANK','MIN','MAX','PRODUCT']:
        out={'SUM':sum(nums),'AVERAGE':sum(nums)/len(nums) if nums else '#DIV/0!','COUNT':len(nums),'COUNTA':sum(x is not None for x in values),'COUNTBLANK':sum(x is None or x=='' for x in values),'MIN':min(nums) if nums else 0,'MAX':max(nums) if nums else 0,'PRODUCT':math.prod(nums) if nums else 0}[fn]
        add('aggregate',f'{fn}_typed_range_{ix}',f'={fn}(A1:A3)',out,[cell(f'A{i+1}',x) for i,x in enumerate(values)],[S['sum'] if fn=='SUM' else url(fn)],rationale=f'{fn} receives {values!r} through a range; stored text/logicals/empty cells differ from numeric zero.')
for err in ['#N/A','#VALUE!','#REF!','#DIV/0!','#NUM!','#NAME?','#NULL!']:
    for fn in ['SUM','AVERAGE','MIN','MAX','PRODUCT','COUNT','COUNTA','COUNTBLANK']:
        out=1 if fn=='COUNT' else 2 if fn=='COUNTA' else 1 if fn=='COUNTBLANK' else err
        add('aggregate',f'{fn}_error_{err}',f'={fn}(A1:A3)',out,[cell('A1',7),cell('A2',err),cell('A3',None)],[S['sum'] if fn=='SUM' else url(fn)],rationale=f'{fn} handles stored {err} alongside a number and genuine blank; counting functions inspect cells rather than propagating every error.')
for text in ['0','-2','3.5','1e2']:
    for b in [True,False]:
        add('aggregate',f'sum_literal_{text}_{b}',f'=SUM("{text}",{str(b).upper()},2)',float(text)+int(b)+2,sources=[S['sum']],rationale='Direct numeric text and logical arguments are coerced, unlike the same values stored in referenced cells.')

# IS predicates deliberately avoid number/text coercion, including Excel errors.
typed=[None,0,-1,1.25,'','0','TRUE',' x ',True,False,'#N/A','#VALUE!','#REF!','#DIV/0!','#NUM!','#NAME?','#NULL!']
for ix,x in enumerate(typed):
    t=v(x)['type']
    for fn in ['ISBLANK','ISNUMBER','ISTEXT','ISNONTEXT','ISLOGICAL','ISERROR','ISERR','ISNA','ISREF']:
        out={'ISBLANK':t=='blank','ISNUMBER':t=='number','ISTEXT':t=='text','ISNONTEXT':t!='text','ISLOGICAL':t=='boolean','ISERROR':t=='error','ISERR':t=='error' and x!='#N/A','ISNA':x=='#N/A','ISREF':True}[fn]
        add('typing',f'{fn}_stored_{ix}',f'={fn}(A1)',out,[cell('A1',x)],[S['is']],rationale=f'{fn} inspects a referenced {t} value {x!r} without converting numeric text or swallowing distinctions among error kinds.')
for fn in ['ISBLANK','ISTEXT','ISNONTEXT','ISNUMBER']:
    out={'ISBLANK':False,'ISTEXT':True,'ISNONTEXT':False,'ISNUMBER':False}[fn]
    add('typing',f'{fn}_formula_empty',f'={fn}(A1)',out,[cell('A1',formula='=""')],[S['is']],rationale='A formula returning empty text is not an empty cell.')

# Branch selection must not evaluate an unselected error-producing expression.
errors=['NA()','1/0','SQRT(-1)','VALUE("bad")']
for cond in ['TRUE','FALSE','0','1','-2']:
    chosen=cond not in ['FALSE','0']
    for err in errors:
        for good in ['17','"kept"','TRUE','""']:
            out={'17':17,'"kept"':'kept','TRUE':True,'""':''}[good]
            formula=f'=IF({cond},{good if chosen else err},{err if chosen else good})'
            add('logic',f'if_lazy_{cond}_{err}_{good}',formula,out,sources=[S['lazy'],url('if')],rationale=f'Condition {cond} selects {good}; the unselected branch {err} would fail if evaluated eagerly.')
for err in ['#N/A','#VALUE!','#REF!','#DIV/0!','#NUM!','#NAME?','#NULL!']:
    for fn in ['IFERROR','IFNA']:
        out='fallback' if fn=='IFERROR' or err=='#N/A' else err
        add('logic',f'{fn}_{err}',f'={fn}(A1,"fallback")',out,[cell('A1',err)],[url(fn)],rationale=f'{fn} distinguishes {err} from the narrower missing-value condition handled by IFNA.')
for a in [True,False,0,2,-2]:
    for b in [True,False,0,2,-2]:
        for fn in ['AND','OR']:
            add('logic',f'{fn}_{a}_{b}',f'={fn}(A1,B1)',(bool(a) and bool(b)) if fn=='AND' else (bool(a) or bool(b)),[cell('A1',a),cell('B1',b)],[url(fn)],rationale='Logical aggregation coerces numeric zero and nonzero values while preserving boolean result type.')

# ASCII positions cover invalid starts, zero lengths, truncation, and overrun.
strings=['','a','ababa',' A  B ','CaseCASE','x*y?z','école','e\u0301']
for ix,text in enumerate(strings):
    for start,length in [(0,1),(1,-1),(1,0),(1,1),(2,1),(2,3),(len(text),2),(len(text)+1,2),(2,99)]:
        out='#VALUE!' if start<1 or length<0 else text[start-1:start-1+length]
        add('text',f'mid_boundary_{ix}_{start}_{length}',f'=MID(A1,{start},{length})',out,[cell('A1',text)],[url('mid')],rationale=f'MID start {start}, count {length}, input length {len(text)} distinguish invalid indices, empty result, and clipping to remaining text.')
    for fn in ['LEFT','RIGHT']:
        for n in [-1,0,1,len(text),len(text)+3]:
            out='#VALUE!' if n<0 else text[:n] if fn=='LEFT' else text[-n:] if n else ''
            add('text',f'{fn}_{ix}_{n}',f'={fn}(A1,{n})',out,[cell('A1',text)],[url(fn)],rationale=f'{fn} count {n} exercises negative, empty, exact-length, or overlong extraction.')
for ix,text in enumerate(['  a  b  ','\u00a0 a \u00a0','a\tb',' a\n b ',' \u2003 ','   ','a\x00b\x1fc','a\x7fb\x85c']):
    add('text',f'trim_spaces_{ix}','=TRIM(A1)',re.sub(' +',' ',text.strip(' ')),[cell('A1',text)],[url('trim')],rationale='TRIM targets ASCII spaces; tabs, line breaks, NBSP, and Unicode spaces survive.')
    add('text',f'clean_controls_{ix}','=CLEAN(A1)',''.join(c for c in text if ord(c)>=32),[cell('A1',text)],[url('clean')],rationale='CLEAN removes controls U+0000 through U+001F and leaves higher Unicode controls and spaces.')
for text in ['banana','ababab','AAAA','aaaa','x*y?z']:
    for needle in ['a','A','ab','*','missing']:
        for fn in ['FIND','SEARCH']:
            for start in [1,2,len(text)+1]:
                # FIND treats wildcards literally; SEARCH wildcard assertions added separately.
                if fn=='SEARCH' and needle=='*': continue
                pos=(text if fn=='FIND' else text.lower()).find(needle if fn=='FIND' else needle.lower(),start-1)
                out=pos+1 if pos>=0 and start<=len(text) else '#VALUE!'
                add('search',f'{fn}_{text}_{needle}_{start}',f'={fn}(B1,A1,{start})',out,[cell('A1',text),cell('B1',needle)],[S['find'] if fn=='FIND' else url('search')],rationale=f'{fn} searches {needle!r} in {text!r} starting at {start}; tests case sensitivity, repeated occurrences, and absent/out-of-range matches.')

# Documented Version 2 text counting is the supported execution contract.
unicode_inputs=['😀','A😀B','😀😀','e\u0301','❤️','👍🏽','👩\u200d💻','𝄞x','a😀e\u0301z','🇺🇸']
for ix,text in enumerate(unicode_inputs):
    seq=list(text)
    compat='Excel / Compatibility Version 2'
    add('unicode',f'len_utf_{ix}_v2','=LEN(A1)',len(seq),[cell('A1',text)],[url('len'),S['compat']],compat,rationale=f'Version 2 counts {len(seq)} units for {text!r}; combining marks, selectors, modifiers, and joiners are separate units.')
    for start,length in [(1,1),(2,1),(2,2),(3,1),(1,99),(len(seq),1),(len(seq)+1,1)]:
        result=seq[start-1:start-1+length]
        out=''.join(result)
        add('unicode',f'mid_utf_{ix}_v2_{start}_{length}',f'=MID(A1,{start},{length})',out,[cell('A1',text)],[url('mid'),S['compat']],compat,rationale=f'Version 2 slices at {start} for {length}; surrogate pairs count once while combining components remain separate.')
    for start,length in [(1,1),(2,1),(2,2),(len(seq)+1,0)]:
        result=seq[:start-1]+['X']+seq[start-1+length:]
        add('unicode',f'replace_utf_{ix}_v2_{start}_{length}',f'=REPLACE(A1,{start},{length},"X")',''.join(result),[cell('A1',text)],[url('replace'),S['compat']],compat,rationale='Replacement positions count surrogate pairs once; insertion at the end preserves preceding characters.')
# Gregorian normalization is modeled independently; Excel's inserted serial 60 is explicit.
epoch=datetime.date(1899,12,31)
def serial(y,m,d):
    y=y+1900 if 0<=y<=1899 else y
    ym=y*12+m-1;yy,mm=divmod(ym,12)
    date=datetime.date(yy,mm+1,1)+datetime.timedelta(days=d-1)
    return (date-epoch).days+int(date>=datetime.date(1900,3,1))
for y in [0,99,100,1899,1900,1904,1999,2000,2100,2400,9999]:
    for m,d in [(1,1),(2,28),(3,0),(2,29),(2,30),(0,1),(13,1),(1,0),(1,32)]:
        if y in [0,1900] and (m,d) in [(2,29),(2,30),(3,0)]:
            out={(2,29):60,(2,30):61,(3,0):60}[(m,d)]
        else:
            try:
                out=serial(y,m,d)
                if out<0:
                    # Avoid inferring an undocumented pre-1900 DATE contract.
                    continue
            except ValueError: out='#NUM!'
        add('dates',f'date_normalize_{y}_{m}_{d}',f'=DATE(A1,B1,C1)',out,[cell('A1',y),cell('B1',m),cell('C1',d)],[url('date'),S['leap']],rationale=f'DATE({y},{m},{d}) exercises short-year conversion, month/day overflow, century leap rules, or the inserted 1900 serial day.')
for y in [-1,10000]:
    add('dates',f'date_invalid_year_{y}',f'=DATE({y},1,1)','#NUM!',sources=[url('date')],rationale='Year outside the documented supported interval produces #NUM!.')

# Lookup position is independently computed; exact duplicate matches choose the first.
tables=[[-5,0,5,10],[1,2,2,4],[0,0,3,9],[-10,-3,-1,0]]
for ix,keys in enumerate(tables):
    cells=[cell(f'A{i+1}',k) for i,k in enumerate(keys)]+[cell(f'B{i+1}',100+i) for i in range(4)]
    probes=sorted(set(keys+[keys[0]-1,keys[-1]+1]+[(a+b)/2 for a,b in zip(keys,keys[1:])]))
    for key in probes:
        for exact in [True,False]:
            if not exact and len(set(keys))!=len(keys): continue
            hits=[i for i,k in enumerate(keys) if k==key] if exact else [i for i,k in enumerate(keys) if k<=key]
            i=(hits[0] if exact else hits[-1]) if hits else None
            add('lookup',f'vlookup_numeric_{ix}_{key}_{exact}',f'=VLOOKUP({key},A1:B4,2,{str(not exact).upper()})','#N/A' if i is None else 100+i,cells,[url('vlookup')],rationale=f'Lookup {key} against sorted {keys}; exact mode requires equality and approximate mode selects the lower boundary.')
            add('lookup',f'match_numeric_{ix}_{key}_{exact}',f'=MATCH({key},A1:A4,{0 if exact else 1})','#N/A' if i is None else i+1,cells,[url('match')],rationale='MATCH returns a relative position; duplicates and probes below/above the sorted key interval distinguish exact and approximate modes.')
for ix,pattern in enumerate(['a*','a?','~*','~?','*z','ALPHA','missing']):
    keys=['alpha','ab','*','?','az','alpha']; cells=[cell(f'A{i+1}',k) for i,k in enumerate(keys)]+[cell(f'B{i+1}',10+i) for i in range(6)]
    expected=[1,2,3,4,5,1,'#N/A'][ix]
    add('lookup',f'match_wildcard_{ix}',f'=MATCH("{pattern}",A1:A6,0)',expected,cells,[url('match')],rationale='Exact MATCH text lookup supports escaped wildcards, case-insensitive equality, and first duplicate selection.')
    add('lookup',f'vlookup_wildcard_{ix}',f'=VLOOKUP("{pattern}",A1:B6,2,FALSE)',expected if isinstance(expected,str) else expected+9,cells,[url('vlookup')],rationale='Exact VLOOKUP wildcard search returns the row value corresponding to the first matching key.')

# Explicit intersections, implicit row/column selection, and reference-valued INDEX.
grid=[cell(f'{chr(65+c)}{r+1}',r*10+c+1) for r in range(6) for c in range(6)]
rects=[(0,0,2,2),(1,1,3,3),(2,0,4,1),(0,3,1,5),(0,0,5,5),(5,5,5,5)]
def rng(t):
    c1,r1,c2,r2=t;return f'{chr(65+c1)}{r1+1}:{chr(65+c2)}{r2+1}'
for i,a in enumerate(rects):
    for j,b in enumerate(rects):
        left,top,right,bottom=max(a[0],b[0]),max(a[1],b[1]),min(a[2],b[2]),min(a[3],b[3])
        vals=[r*10+c+1 for r in range(top,bottom+1) for c in range(left,right+1)]
        out=sum(vals) if vals else '#NULL!'
        add('references',f'explicit_intersect_{i}_{j}',f'=SUM({rng(a)} {rng(b)})',out,grid,[S['operators']],rationale=f'The space operator intersects {rng(a)} with {rng(b)}; disjoint rectangles yield #NULL! rather than an empty numeric sum.')
for row in range(1,8):
    for form in ['@A1:A6','A1:A6+0']:
        modern=form.startswith('@')
        add('references',f'implicit_vertical_{row}_{modern}','='+form,row*10-9 if row<=6 else '#VALUE!',grid,[S['intersection']],'Excel dynamic arrays / Compatibility Version 1' if modern else 'Excel legacy / Compatibility Version 1',target=f'Z{row}',rationale='Implicit intersection chooses the range cell on the formula row; a formula outside the range has no unique intersection.')
for col in range(1,8):
    add('references',f'implicit_horizontal_{col}','=@A1:F1',col if col<=6 else '#VALUE!',grid,[S['intersection']],'Excel dynamic arrays / Compatibility Version 1',target=f'{chr(64+col)}10',rationale='Explicit @ on a horizontal range selects the cell in the formula column, and fails beyond the range.')
for row in [0,1,2,6,7]:
    for col in [0,1,2,6,7]:
        if row>6 or col>6: out='#REF!'
        else:
            rs=range(6) if row==0 else [row-1]
            cs=range(6) if col==0 else [col-1]
            out=sum(r*10+c+1 for r in rs for c in cs)
        add('references',f'index_bounds_{row}_{col}',f'=SUM(INDEX(A1:F6,{row},{col}))',out,grid,[url('index')],rationale='INDEX zero indices return a complete row or column reference; positive indices outside the rectangular source range return #REF!. SUM consumes the returned reference.')

# Round-robin balances the initial independently derived baseline across categories.
chosen=[]
for category,items in groups.items():
    random.Random('excel-source-fixtures-'+category).shuffle(items)
while len(chosen)<1000:
    progress=False
    for cat,items in groups.items():
        if items and len(chosen)<1000:
            chosen.append(items.pop(0));progress=True
    if not progress: raise RuntimeError(f'Only {len(chosen)} cases')
seen=set()
def signature(spec):
    return json.dumps([spec['cells'],spec['target'],spec['expected']],sort_keys=True)
for i,spec in enumerate(chosen):
    while signature(spec) in seen:
        spec=groups[spec['category']].pop(0)
        chosen[i]=spec
    seen.add(signature(spec))
for i,spec in enumerate(chosen):
    spec['id']=i

    spec['name']=re.sub('[^a-z0-9]+','_',spec['name'].lower()).strip('_')+f'_{i:04d}'
assert len({s['name'] for s in chosen})==1000
from extend import extend
extend(chosen,add,cell,url,groups,signature)

used=sorted({u for s in chosen for u in s['source_urls']})
applicability={S['operators']:'Reference intersection syntax and common-cell semantics.',S['intersection']:'Scalarization of ranges by formula row or column, and explicit @ syntax.',S['compat']:'Workbook Versions 1/2 and surrogate-pair counting changes; combining components remain separate.',S['lazy']:'Microsoft Evaluate Formula example explicitly leaves the unselected IF branch unevaluated.',S['sum']:'Direct argument coercion versus nonnumeric referenced-cell exclusion.',S['is']:'Type predicates do not coerce numeric text; blank and error-kind distinctions.',S['find']:'Case-sensitive search positions and Compatibility Version 2 counting.',S['leap']:'Excel inserts a fictitious leap day in 1900; other century leap rules remain Gregorian.',S['round']:'Half-away-from-zero, toward-zero, and away-from-zero decimal rounding.'}
sources=[{'url':u,'publisher':'Microsoft','verified_on':'2026-10-13','applicability':applicability.get(u,'Documented '+u.rsplit('/',1)[-1].replace('-function','').upper()+' syntax, input boundaries, reference handling, and returned Excel result or error.'),'spec_ids':[s['id'] for s in chosen if u in s['source_urls']]} for u in used]

print(json.dumps({'total':len(chosen),'categories':dict(collections.Counter(s['category'] for s in chosen)),'sources':len(used)}))

# Inventory mapping and curated scenarios are assembled after the independent baseline.
from assemble import assemble
chosen = assemble(chosen, sources)
print(json.dumps({"assembled_total": len(chosen)}))
