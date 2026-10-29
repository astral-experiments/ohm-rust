"""Recompute every whole-rustc paired effect and optionally reopen the original native profiles."""
import argparse
import gzip
import hashlib
import itertools
import json
import math
from pathlib import Path
import re
import statistics

parser = argparse.ArgumentParser()
parser.add_argument('--raw', action='append', default=[], metavar='WORKLOAD=DIR')
parser.add_argument('--packet', type=Path, default=Path(__file__).resolve().parent)
args = parser.parse_args()
report = json.loads((args.packet / 'evidence.json').read_bytes())
raw = {k: Path(v) for k,v in (entry.split('=', 1) for entry in args.raw)}

def digest(data):
    return hashlib.sha256(data).hexdigest()

def verify(s):
    slots = s['slots']
    order = s['order']
    assert order.count('AB') == order.count('BA') and len(slots) == 2 * len(order)
    values = []
    diffs = []
    for index,layout in enumerate(order, 1):
        a,b = (slots[f'{index}-{arm}'] for arm in 'AB')
        assert a['metadata'] == b['metadata'] == s['common_command_metadata']
        for item,arm in ((a,'A'),(b,'B')):
            assert item['ir'] > 10_000_000 and 0 < item['decoder_inclusive'] < item['ir']
            if s['root'] in raw:
                body = gzip.decompress((raw[s['root']] / f'slot-{index}-{arm}' / 'rustc.callgrind.gz').read_bytes())
                assert len(body) == item['bytes'] and digest(body) == item['raw_sha256']
                count, = re.findall(rb'^totals: (\d+)\s*$', body, re.M)
                assert int(count) == item['ir'] and b'\nevents: Ir\n' in body
        values.append(math.log(b['ir'] / a['ir']))
        diffs.append(b['ir'] - a['ir'])
    med = statistics.median(values)
    chrono = [v if o=='AB' else -v for v,o in zip(values,order)]
    perms = [statistics.median(v if j in forward else -v for j,v in enumerate(chrono)) for forward in itertools.combinations(range(len(order)),len(order)//2)]
    pv = sum(abs(v) >= abs(med)-1e-15 for v in perms)/len(perms)
    return {'pairs':len(values),'whole_root_percent':math.expm1(med)*100,'median_ir':statistics.median(diffs),'negative':sum(x<0 for x in diffs),'two_sided_balanced_P':pv,'permutations':len(perms),'inclusive_decoder_percent':statistics.median((slots[f'{i}-B']['decoder_inclusive']/slots[f'{i}-A']['decoder_inclusive']-1)*100 for i in range(1,len(order)+1))}

assert report['code']['differing_sdk_file_count']==1 and report['code']['literal_bootstrap_commands']==119 and report['code']['identical_std_members']==53
assert report['wire']['stream_test_cases']==65792 and len(report['wire']['historical_graphs'])==9
for experiment in report['experiments']:
    recalc=verify(experiment); root=experiment['root']
    if len(experiment['order'])==12:
        assert len(experiment['order'])==12 and recalc['permutations']==924
        accept=recalc['negative']>=10 and recalc['whole_root_percent']<0 and recalc['two_sided_balanced_P']<.05
        assert accept==experiment['registered_decision']
    print(root,json.dumps(recalc,sort_keys=True))
excluded=report['excluded_parser']
assert excluded['profile_pairs']==0 and excluded['reported_rmetas']==72 and excluded['identical']==65 and len(excluded['different'])==7
assert excluded['initial']['native']==excluded['initial']['repeat']!=excluded['initial']['clean']
assert excluded['original_program']=='accepted\n' and excluded['edited_program']=='rejected: Star import cannot be parenthesized\n'
for group in report['startup'].values():
    d=[group['rows'][f'{i}-B']-group['rows'][f'{i}-A'] for i in range(1,5)]
    assert statistics.median(d)==group['median']
print('Parser gate excluded, and all 16 separate compiler startup controls verified.')
