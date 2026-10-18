"""Independently verify case 1513 from Microsoft's defining Bessel series.

No ROI, libm, native Excel, or external numeric library supplies the oracle.
For integer order n, Gamma(n+k+1) = (n+k)!, so successive terms satisfy
t_0 = (x/2)^n/n! and t_k = -t_(k-1)*(x/2)^2/(k*(n+k)).
"""
import json
from decimal import Decimal, localcontext
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
import sys
sys.path.insert(0, str(ROOT / 'test-specs'))
from case_table import read_table



def series(precision):
    with localcontext() as context:
        context.prec = precision
        half_x = Decimal('1.9') / 2
        term = half_x ** 2 / 2
        result = term
        for k in range(1, 100):
            term *= -(half_x ** 2) / Decimal(k * (k + 2))
            result += term
        # Absolute terms decrease: each ratio is 0.9025/(k*(k+2)) < 1.
        # The alternating-series remainder is bounded by the next term.
        remainder_bound = abs(term * half_x ** 2 / Decimal(100 * 102))
        return result, term, remainder_bound


def main():
    value70, term70, bound70 = series(70)
    value90, _, _ = series(90)
    assert abs(value70 - value90) < Decimal('1e-69')
    assert bound70 < Decimal('1e-324')
    cases = read_table(ROOT)
    case = next(case for case in cases if case['id'] == 1513)
    assert case['cells'][0]['formula'] == '=BESSELJ(1.9,2)'
    assertion = case['steps'][0]['assertions'][0]
    assert assertion['expected'] == {'type': 'number', 'value': float(value90)}
    assert assertion['tolerance'] == 5e-10  # Retain the original absolute tolerance.
    assert assertion['evidence']['kind'] == 'documented_derivation'
    assert str(value70) in assertion['evidence']['derivation']
    assert abs(Decimal('0.329925829') - value90) > Decimal('5e-10')
    print(f'Case 1513 defining series (70 digits, 100 terms): {value70}')
    print(f'Final term: {term70}; alternating remainder bound: {bound70}')
    print('Independent 90-digit calculation agrees; fixture and evidence verified.')


if __name__ == '__main__':
    main()
