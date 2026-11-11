"""Small offline quality harness primitives. Inputs are private; reports contain aggregates only."""
import argparse
import json
import math
import os
from pathlib import Path
import sys
import unicodedata


def require(condition):
    if not condition:
        raise ValueError('invalid_evaluation_manifest')


def object_keys(value, required, optional=()):
    require(isinstance(value, dict) and set(required) <= set(value) and set(value) <= set(required) | set(optional))


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def text(value, limit=20000, empty=True):
    require(isinstance(value, str) and len(value) <= limit and (empty or bool(value)))
    return value


def ratio(numerator, denominator):
    return numerator / denominator if denominator else None


def normalized(value):
    return unicodedata.normalize('NFKC', value).casefold()


def edit_distance(reference, prediction):
    # Trim identical edges before exact Levenshtein; memory stays linear in the shorter input.
    begin = 0
    while begin < min(len(reference), len(prediction)) and reference[begin] == prediction[begin]:
        begin += 1
    reference, prediction = reference[begin:], prediction[begin:]
    while reference and prediction and reference[-1] == prediction[-1]:
        reference, prediction = reference[:-1], prediction[:-1]
    if len(reference) < len(prediction):
        reference, prediction = prediction, reference
    previous = list(range(len(prediction) + 1))
    for i, a in enumerate(reference, 1):
        current = [i]
        for j, b in enumerate(prediction, 1):
            current.append(min(current[-1] + 1, previous[j] + 1, previous[j - 1] + (a != b)))
        previous = current
    return previous[-1]


def load_manifest(path, task, metrics):
    require(path.is_file() and not path.is_symlink() and not path.stat().st_mode & 0o077 and path.stat().st_size <= 32 * 1024 * 1024)
    value = json.loads(path.read_text(), parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite_json')))
    object_keys(value, ('version', 'task', 'samples'), ('thresholds',))
    require(type(value['version']) is int and value['version'] == 1 and value['task'] == task)
    require(isinstance(value['samples'], list) and 1 <= len(value['samples']) <= 1000)
    thresholds = value.get('thresholds', {})
    require(isinstance(thresholds, dict) and set(thresholds) <= set(metrics))
    for name, rule in thresholds.items():
        require(isinstance(rule, dict) and set(rule) in ({'min'}, {'max'}))
        bound = next(iter(rule.values()))
        require(number(bound) and bound >= 0)
    return value


def finish(task, sample_count, reference_count, metrics, counts, thresholds):
    checks = {}
    for name, rule in thresholds.items():
        actual = metrics[name]
        comparator, expected = next(iter(rule.items()))
        passed = None if actual is None else actual >= expected if comparator == 'min' else actual <= expected
        checks[name] = {'value': actual, comparator: expected, 'passed': passed}
    decisions = [check['passed'] for check in checks.values()]
    passed = False if False in decisions else None if not decisions or None in decisions or reference_count != sample_count else True
    return {'version': 1, 'task': task, 'samples': sample_count, 'reference_samples': reference_count,
            'missing_reference_samples': sample_count - reference_count, 'metrics': metrics, 'counts': counts,
            'thresholds': checks, 'acceptance_passed': passed,
            'scope': 'offline provided references; no external requests; no live quality claim'}


def main(task, metric_names, evaluate):
    parser = argparse.ArgumentParser(description='Evaluate a private local manifest; output only aggregate metrics.')
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    try:
        require(args.manifest.resolve() != args.report.resolve())
        require(not (args.report.exists() and os.path.samefile(args.manifest, args.report)))
        manifest = load_manifest(args.manifest, task, metric_names)
        report = evaluate(manifest)
        args.report.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        fd = os.open(args.report, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, 'O_NOFOLLOW', 0), 0o600)
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, 'w') as stream:
            json.dump(report, stream, ensure_ascii=False, allow_nan=False, indent=2)
            stream.write('\n')
        print(json.dumps(report, ensure_ascii=False, allow_nan=False))
        return 2 if report['acceptance_passed'] is False else 0
    except (OSError, ValueError, TypeError, KeyError, RuntimeError):
        print('{"error":"invalid_evaluation_manifest_or_report"}', file=sys.stderr)
        return 1
