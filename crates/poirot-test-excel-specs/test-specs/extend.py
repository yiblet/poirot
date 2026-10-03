"""Additional source-derived coverage; called by generate.py without ROI imports."""
import math, re, json, random, datetime, collections

def extend(chosen, add, cell, url, groups, signature):
    groups.clear()
    def source(fn): return url(fn.replace('.','-'))
    # Criteria matrices separate relational boundaries, conjunctions, and selection.
    for ix,keys in enumerate([[-3,-1,0,1,3], [0,0,1,1,4],[-5,2,2,7,9]]):
        sums=[-7,0,5,11,20]
        cells=[cell(f'A{i+1}',k) for i,k in enumerate(keys)]+[cell(f'B{i+1}',n) for i,n in enumerate(sums)]
        for threshold in [-1,0,2,10]:
            for op,predicate in [('=',lambda x,t:x==t),('<>',lambda x,t:x!=t),('<',lambda x,t:x<t),('<=',lambda x,t:x<=t),('>',lambda x,t:x>t),('>=',lambda x,t:x>=t)]:
                idx=[i for i,k in enumerate(keys) if predicate(k,threshold)]
                criteria=f'{op}{threshold}'
                for fn in ['COUNTIF','SUMIF','AVERAGEIF']:
                    out=len(idx) if fn=='COUNTIF' else sum(sums[i] for i in idx) if fn=='SUMIF' else sum(sums[i] for i in idx)/len(idx) if idx else '#DIV/0!'
                    formula=f'={fn}(A1:A5,"{criteria}"'+(')' if fn=='COUNTIF' else ',B1:B5)')
                    src='https://support.microsoft.com/en-us/excel/count-numbers-or-dates-based-on-a-condition-in-excel' if fn=='COUNTIF' else source(fn)
                    add('criteria',f'{fn}_relation_{ix}_{op}_{threshold}',formula,out,cells,[src],rationale=f'{fn} relational criterion {criteria} selects positions {idx} from {keys}; the sum/average range includes negative, zero, and positive amounts.')
        for low,high in [(-3,0),(0,0),(0,3),(2,10),(10,2)]:
            idx=[i for i,k in enumerate(keys) if k>=low and k<=high]
            for fn in ['COUNTIFS','SUMIFS']:
                formula=f'={fn}('+('B1:B5,' if fn=='SUMIFS' else '')+f'A1:A5,">={low}",A1:A5,"<={high}")'
                add('criteria',f'{fn}_interval_{ix}_{low}_{high}',formula,len(idx) if fn=='COUNTIFS' else sum(sums[i] for i in idx),cells,[source(fn)],rationale='Multiple criteria are conjoined per row; equal endpoints and contradictory intervals exercise boundary selection.')
    for ix,pattern in enumerate(['a*','a?','~*','~?','*z','ALPHA','missing']):
        keys=['alpha','ab','*','?','az','alpha']; hits=[[0,1,4,5],[1,4],[2],[3],[4],[0,5],[]][ix]
        cells=[cell(f'A{i+1}',k) for i,k in enumerate(keys)]+[cell(f'B{i+1}',i-2) for i in range(6)]
        for fn in ['SUMIF','AVERAGEIF']:
            out=sum(i-2 for i in hits) if fn=='SUMIF' else sum(i-2 for i in hits)/len(hits) if hits else '#DIV/0!'
            add('criteria',f'{fn}_wildcard_{ix}',f'={fn}(A1:A6,"{pattern}",B1:B6)',out,cells,[source(fn)],rationale='Wildcard and escaped-literal criteria select all matching rows, unlike a first-match lookup.')
    for fn in ['SUMIFS','COUNTIFS']:
        add('criteria',f'{fn}_mismatched_ranges',f'={fn}('+('B1:B3,' if fn=='SUMIFS' else '')+'A1:A3,">0",C1:C2,"x")','#VALUE!',[cell('A1',1),cell('C1','x')],[source(fn)],rationale='Criteria range dimensions must match; a shorter second range is rejected.')

    # Statistics use sample-size, tie, parity, and interpolation boundaries.
    datasets=[[1],[0,0],[1,3],[3,1,2],[1,2,9,10],[-5,-1,0,8,8],[1,'2',True,None,3],['text',None,False],[4,4,4],[-2.5,0,2.5,7.5]]
    for ix,data in enumerate(datasets):
        nums=sorted(x for x in data if isinstance(x,(int,float)) and not isinstance(x,bool))
        cells=[cell(f'A{i+1}',x) for i,x in enumerate(data)]; ref=f'A1:A{len(data)}'
        for fn in ['MEDIAN','VAR.S','VAR.P','STDEV.S','STDEV.P']:
            if fn=='MEDIAN':
                out=(nums[(len(nums)-1)//2]+nums[len(nums)//2])/2 if nums else 0
            elif len(nums)<(2 if fn.endswith('.S') else 1): out='#DIV/0!'
            else:
                variance=sum((n-sum(nums)/len(nums))**2 for n in nums)/(len(nums)-int(fn.endswith('.S')))
                out=math.sqrt(variance) if fn.startswith('STDEV') else variance
            formula=f'={fn}({ref}'+(',0)' if fn=='MEDIAN' and not nums else ')')
            add('statistics',f'{fn}_typed_sample_{ix}',formula,out,cells,[source(fn)],rationale=f'{fn} handles sample size {len(nums)}, sorted values {nums}, parity, ties, and referenced nonnumeric exclusions.'+(' Explicit numeric0 supplies the only eligible MEDIAN value.' if fn=='MEDIAN' and not nums else ''))
        for k in [-0.1,0,0.25,0.5,0.75,1,1.1]:
            if not nums or k<0 or k>1: out='#NUM!'
            else:
                p=k*(len(nums)-1); lo=math.floor(p); hi=math.ceil(p)
                out=nums[lo]+(p-lo)*(nums[hi]-nums[lo])
            add('statistics',f'percentile_{ix}_{k}',f'=PERCENTILE.INC({ref},{k})',out,cells,[source('PERCENTILE.INC')],rationale='Inclusive percentile endpoints, off-grid interpolation, out-of-domain probabilities, and empty numeric samples are distinct contracts.')
        if nums:
            for number in sorted(set(nums)):
                for order in [0,1,-1]:
                    out=1+sum(n>number if order==0 else n<number for n in nums)
                    add('statistics',f'rank_ties_{ix}_{number}_{order}',f'=RANK.EQ({number},{ref},{order})',out,cells,[source('RANK.EQ')],rationale='Ties share the top rank; any nonzero ordering argument selects ascending ranks and gaps remain after duplicates.')
            for q in [-1,0,1,2,3,4,5,2.9]:
                qq=math.trunc(q)
                if qq<0 or qq>4: out='#NUM!'
                else:
                    p=qq/4*(len(nums)-1);lo=math.floor(p);hi=math.ceil(p);out=nums[lo]+(p-lo)*(nums[hi]-nums[lo])
                add('statistics',f'quartile_{ix}_{q}',f'=QUARTILE.INC({ref},{q})',out,cells,[source('QUARTILE.INC')],rationale='Quartile truncates fractional selectors, includes minimum/maximum, and rejects selectors outside 0..4.')

    # Financial expectations follow the documented constant-payment cash-flow equation.
    for rate in [-0.1,0,0.01,0.1]:
        for nper in [1,2,12]:
            for timing in [0,1]:
                for pv,fv in [(100,0),(-100,25),(0,100)]:
                    factor=(1+rate)**nper
                    annuity=nper if rate==0 else (factor-1)/rate
                    pmt=-(pv*factor+fv)/((1+rate*timing)*annuity)
                    add('financial',f'pmt_cashflow_{rate}_{nper}_{timing}_{pv}_{fv}',f'=PMT(A1,B1,C1,D1,E1)',pmt,[cell('A1',rate),cell('B1',nper),cell('C1',pv),cell('D1',fv),cell('E1',timing)],[source('PMT')],rationale='Payment timing, zero/negative interest, one-period terms, terminal balances, and cash-flow sign are independently calculated from the annuity equation.')
                    payment=-10
                    future=-(pv*factor+payment*(1+rate*timing)*annuity)
                    present=-(fv+payment*(1+rate*timing)*annuity)/factor
                    add('financial',f'fv_cashflow_{rate}_{nper}_{timing}_{pv}',f'=FV({rate},{nper},{payment},{pv},{timing})',future,sources=[source('FV')],rationale='Future value includes accumulation of principal and beginning/end period constant payments; zero rate uses the linear limit.')
                    add('financial',f'pv_cashflow_{rate}_{nper}_{timing}_{fv}',f'=PV({rate},{nper},{payment},{fv},{timing})',present,sources=[source('PV')],rationale='Present value discounts the terminal balance and annuity; payment timing changes the annuity factor.')
    for fn in ['PMT','PV','FV']:
        add('financial',f'{fn}_nonnumeric_rate',f'={fn}("bad",2,-10,100,0)','#VALUE!',sources=[source(fn)],rationale='A nonnumeric rate cannot participate in the financial cash-flow equation.')

    # AND/OR ignore text and blanks in references, unlike numeric zero.
    logical_data=[[True,'x',None],[False,'x',None],[1,'0',None],[0,'1',None],['x','1',None],[True,False,''],[0,2,False],[None,None,None]]
    for ix,data in enumerate(logical_data):
        eligible=[x for x in data if isinstance(x,(int,float,bool))]
        for fn in ['AND','OR']:
            out='#VALUE!' if not eligible else all(eligible) if fn=='AND' else any(eligible)
            add('logical_coercion',f'{fn}_reference_typing_{ix}',f'={fn}(A1:A3)',out,[cell(f'A{i+1}',x) for i,x in enumerate(data)],[source(fn)],rationale='Logical ranges ignore stored text and genuine blanks; no eligible logical/numeric values yields #VALUE!.')
            add('logical_coercion',f'{fn}_reference_and_literal_{ix}',f'={fn}(A1:A3,FALSE)',False if fn=='AND' else any(eligible),[cell(f'A{i+1}',x) for i,x in enumerate(data)],[source(fn)],rationale='Direct FALSE supplies an eligible value; referenced text/blank cells are ignored and numeric/logical cells participate.')
    for err in ['#N/A','#VALUE!','#REF!','#DIV/0!','#NUM!','#NAME?','#NULL!']:
        for fn in ['AND','OR']:
            for decisive in [True,False]:
                add('logical_coercion',f'{fn}_eager_error_{err}_{decisive}',f'={fn}(A1,A2)',err,[cell('A1',decisive),cell('A2',err)],[source(fn)],rationale='Logical aggregation still sees an error after a decisive boolean; it does not use IF-style branch selection.')

    # Real Gregorian weekday calculation is safe after Excel's 1900 anomaly.
    epoch=datetime.date(1899,12,31)
    def serial(date): return (date-epoch).days+int(date>=datetime.date(1900,3,1))
    starts=[datetime.date(2024,2,28),datetime.date(2024,2,29),datetime.date(2024,3,1),datetime.date(2024,3,2),datetime.date(2024,3,3),datetime.date(2024,12,31),datetime.date(2100,2,28)]
    for date in starts:
        wd=date.weekday()
        for kind in [1,2,3,11,12,13,14,15,16,17,0,4]:
            out=(wd+1)%7+1 if kind==1 else wd+1 if kind in [2,11] else wd if kind==3 else (wd-(kind-11))%7+1 if 12<=kind<=17 else '#NUM!'
            add('calendar',f'weekday_{date}_{kind}',f'=WEEKDAY(A1,{kind})',out,[cell('A1',serial(date))],[source('WEEKDAY')],rationale=f'{date} weekday {wd} is mapped to return-type {kind}; alternate week origins and invalid modes are distinct.')
        for days in [-5,-1,0,1,5,1.9,-1.9]:
            for use_holidays in [False,True]:
                holidays={datetime.date(2024,2,29),datetime.date(2024,3,4)} if use_holidays else set()
                if days==0 and (date.weekday()>=5 or date in holidays): continue
                remaining=abs(math.trunc(days));result=date;step=1 if days>=0 else -1
                while remaining:
                    result+=datetime.timedelta(days=step)
                    if result.weekday()<5 and result not in holidays: remaining-=1
                cells=[cell('A1',serial(date))]+([cell('B1',serial(datetime.date(2024,2,29))),cell('B2',serial(datetime.date(2024,3,4)))] if use_holidays else [])
                add('calendar',f'workday_{date}_{days}_{use_holidays}',f'=WORKDAY(A1,{days}'+(',B1:B2)' if use_holidays else ')'),serial(result),cells,[source('WORKDAY')],rationale='Signed business-day offsets truncate fractional counts, skip weekends/holidays, and cross leap-day/year boundaries; zero offset preserves the starting date.')

    # References include quoted sheet names, dynamic text, geometry, and lazy CHOOSE.
    grid=[cell(f'{chr(65+c)}{r+1}',r*10+c+1) for r in range(6) for c in range(6)]
    for dr in [-2,-1,0,1,3,5]:
        for dc in [-2,-1,0,1,3,5]:
            for height,width in [(1,1),(2,2),(3,1)]:
                rr,cc=2+dr,2+dc
                out='#REF!' if rr<0 or cc<0 else sum(r*10+c+1 if r<6 and c<6 else 0 for r in range(rr,rr+height) for c in range(cc,cc+width))
                add('dynamic_references',f'offset_geometry_{dr}_{dc}_{height}_{width}',f'=SUM(OFFSET(C3,{dr},{dc},{height},{width}))',out,grid,[source('OFFSET')],rationale='OFFSET preserves reference semantics and rectangular shape; above/left of the worksheet produces #REF!, while unwritten cells in valid geometry are blank.')
    for ix,text in enumerate(['A1','$A$1','A1:B2','A0','XFE1','A1048577','Missing!A1',"'Data sheet'!B2",'R2C2']):
        for a1 in [True,False]:
            valid={'A1':1,'$A$1':1,'A1:B2':26,"'Data sheet'!B2":99} if a1 else {'R2C2':12}
            out=valid.get(text,'#REF!')
            cells=grid+[cell('A10',text),cell('B2',99,sheet=1)]
            add('dynamic_references',f'indirect_syntax_{ix}_{a1}',f'=SUM(INDIRECT(A10,{str(a1).upper()}))',out,cells,[source('INDIRECT')],sheets=['Sheet1','Data sheet'],rationale='INDIRECT resolves textual A1/R1C1 addresses including absolute syntax, quoted sheets, range references, and invalid worksheet bounds.')
    for ix in [-1,0,1,1.9,2,2.9,3,4]:
        choice=math.trunc(ix)
        out='#VALUE!' if choice<1 or choice>3 else [21,26,12][choice-1]
        add('dynamic_references',f'choose_reference_{ix}',f'=SUM(CHOOSE({ix},A3,A1:B2,B2))',out,grid,[source('CHOOSE')],rationale='CHOOSE truncates fractional indices, rejects out-of-range indices, and returns a selected reference that SUM can consume.')
    for ix in [1,2,3]:
        vals=['1/0','NA()','SQRT(-1)'];vals[ix-1]='17'
        add('dynamic_references',f'choose_lazy_{ix}',f'=CHOOSE({ix},'+','.join(vals)+')',17,sources=[source('CHOOSE'),'https://support.microsoft.com/en-us/excel/detect-formula-errors-in-excel'],rationale='Only the selected CHOOSE argument supplies the value; other alternatives would produce different Excel errors if evaluated eagerly.')

    # Reserve formula dependencies rather than selecting only literal-input matrices.
    for fn in ['SUMIF','COUNTIF','AVERAGEIF','SUMIFS','COUNTIFS']:
        cells=[cell('A1',-2),cell('A2',formula='=A1+3'),cell('A3',formula='=A2*2'),cell('B1',formula='=A1*5'),cell('B2',formula='=A2*5'),cell('B3',formula='=B2*2')]
        if fn=='COUNTIF':formula='=COUNTIF(A1:A3,">0")';out=2
        elif fn=='COUNTIFS':formula='=COUNTIFS(A1:A3,">0",B1:B3,"<10")';out=1
        elif fn=='SUMIFS':formula='=SUMIFS(B1:B3,A1:A3,">0",B1:B3,"<10")';out=5
        else:formula=f'={fn}(A1:A3,">0",B1:B3)';out=15 if fn=='SUMIF' else 7.5
        src='https://support.microsoft.com/en-us/excel/count-numbers-or-dates-based-on-a-condition-in-excel' if fn=='COUNTIF' else source(fn)
        add('criteria',f'chain_{fn}_computed_keys',formula,out,cells,[src],rationale=f'Dependency chain produces keys [-2,1,2] and amounts [-10,5,10]; row criteria select a documented total/count/mean of {out}.')
    for fn in ['SUMIF','AVERAGEIF']:
        add('criteria',f'chain_{fn}_short_amount_reference',f'={fn}(A1:A3,">0",B1)',15 if fn=='SUMIF' else 7.5,[cell('A1',-1),cell('A2',1),cell('A3',2),cell('B1',formula='=""'),cell('B2',5),cell('B3',10)],[source(fn)],rationale='The amount/average range begins at B1 and expands to criterion-range shape; selected B2/B3 total15 and mean7.5 even though only B1 was passed.')
    for fn in ['PMT','PV','FV']:
        args={'PMT':('=PMT(A1,A2,A3,A4,A5)',-50),'PV':('=PV(A1,A2,-50,A4,A5)',100),'FV':('=FV(A1,A2,-50,A3,A5)',0)}[fn]
        cells=[cell('A1',formula='=1-1'),cell('A2',formula='=1+1'),cell('A3',formula='=25*4'),cell('A4',formula='=0*1'),cell('A5',formula='=1')]
        add('financial',f'chain_{fn}_zero_rate',args[0],args[1],cells,[source(fn),source('PV')],rationale='Computed zero-rate cash flow satisfies pmt*nper+pv+fv=0: -50*2+100+0=0, independent of beginning/end timing.')
    for ix,formula in enumerate(['=SUM(INDIRECT(A10))','=SUM(OFFSET(C3,A10,A11,2,2))','=SUM(CHOOSE(A10,A1:A2,B1:B2))']):
        cells=grid+[cell('A10',formula='="A"&"1:B2"' if ix==0 else '=1+1' if ix==2 else '=1-1'),cell('A11',formula='=1-1')]
        out=[26,114,14][ix]
        add('dynamic_references',f'chain_reference_{ix}',formula,out,cells,[source(['INDIRECT','OFFSET','CHOOSE'][ix])],rationale=f'Computed text or index dependencies select a reference consumed by SUM; independently summing selected cells yields {out}.')
    add('calendar','chain_leap_workday','=WORKDAY(A1,A2)',45352,[cell('A1',formula='=DATE(2024,2,28)'),cell('A2',formula='=1+1')],[source('WORKDAY'),source('DATE')],rationale='DATE computes serial45350 for Wednesday2024-02-28; two business days land on Friday2024-03-01, serial45352.')

    quotas={'criteria':100,'statistics':100,'financial':100,'logical_coercion':50,'calendar':70,'dynamic_references':80}
    seen={signature(s) for s in chosen}
    for cat,quota in quotas.items():
        candidates=groups[cat]
        random.Random('expanded-excel-'+cat).shuffle(candidates)
        candidates.sort(key=lambda s:not s['name'].startswith('chain_'))
        selected=[]
        for spec in candidates:
            key=signature(spec)
            if key in seen: continue
            seen.add(key);selected.append(spec)
            if len(selected)==quota: break
        assert len(selected)==quota,(cat,len(selected),quota)
        for spec in selected:
            i=len(chosen);spec['id']=i
            spec['name']=re.sub('[^a-z0-9]+','_',spec['name'].lower()).strip('_')+f'_{i:04d}'
            spec['text_policy']='excel_v1'
            if spec['category']=='financial' and spec['expected']['type']=='number': spec['tolerance']=1e-8
            chosen.append(spec)
