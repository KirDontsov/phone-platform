#!/usr/bin/env python3
"""Offline analysis/reference alignment; no LLM requests or dialogue text in reports."""
from evaluation import finish, main, number, object_keys, ratio, require, text

METRICS = ('status_accuracy', 'tag_precision', 'tag_recall', 'criterion_id_precision', 'criterion_id_recall',
           'criterion_status_accuracy', 'criterion_score_mae', 'criterion_score_coverage', 'evidence_precision', 'evidence_recall',
           'evidence_validity', 'abstention_precision', 'abstention_recall', 'reference_coverage')


def evidence(value):
    object_keys(value, ('segment_id', 'start', 'end', 'quote'))
    text(value['segment_id'], 200, False); text(value['quote'], 20000, False)
    require(number(value['start']) and number(value['end']) and 0 <= value['start'] <= value['end'] <= 86400)
    return value


def analysis(value):
    object_keys(value, ('criteria', 'status_id', 'tag_ids', 'abstained'))
    require(value['status_id'] is None or isinstance(value['status_id'], str))
    if value['status_id'] is not None:
        text(value['status_id'], 200, False)
    require(type(value['abstained']) is bool and isinstance(value['tag_ids'], list) and len(value['tag_ids']) <= 30)
    for tag in value['tag_ids']:
        text(tag, 200, False)
    require(len(set(value['tag_ids'])) == len(value['tag_ids']))
    require(isinstance(value['criteria'], list) and len(value['criteria']) <= 100)
    seen = set()
    for criterion in value['criteria']:
        object_keys(criterion, ('id', 'status', 'score', 'evidence'))
        text(criterion['id'], 200, False)
        require(criterion['id'] not in seen); seen.add(criterion['id'])
        require(criterion['status'] in ('scored', 'not_applicable', 'insufficient_evidence'))
        score = criterion['score']
        require((criterion['status'] == 'scored' and number(score) and 0 <= score <= 5)
                or (criterion['status'] != 'scored' and score is None))
        require(isinstance(criterion['evidence'], list) and len(criterion['evidence']) <= 100)
        for quote in criterion['evidence']:
            evidence(quote)
    return value


def citations(value):
    return {(criterion['id'], quote['segment_id'], quote['start'], quote['end'], quote['quote'])
            for criterion in value['criteria'] for quote in criterion['evidence']}


def segments(value):
    require(isinstance(value, list) and len(value) <= 10000)
    result = {}
    for segment in value:
        object_keys(segment, ('id', 'start', 'end', 'text'))
        text(segment['id'], 200, False); text(segment['text'])
        require(segment['id'] not in result and number(segment['start']) and number(segment['end'])
                and 0 <= segment['start'] <= segment['end'] <= 86400)
        result[segment['id']] = segment
    return result


def evaluate(manifest):
    refs = statuses = correct_status = pred_tags = ref_tags = match_tags = 0
    pred_criteria = ref_criteria = match_criteria = criterion_correct = criterion_total = 0
    reference_scored = 0
    score_error = 0.0; score_pairs = pred_evidence = ref_evidence = match_evidence = 0
    evidence_checked = evidence_valid = pred_abstain = ref_abstain = match_abstain = 0
    for sample in manifest['samples']:
        object_keys(sample, ('prediction', 'reference'), ('segments',))
        prediction = analysis(sample['prediction'])
        source = segments(sample['segments']) if 'segments' in sample else None
        # Evidence validity is factual alignment with provided text, separate from human citation recall.
        if source is not None:
            for _, segment_id, start, end, quote in citations(prediction):
                evidence_checked += 1
                segment = source.get(segment_id)
                evidence_valid += bool(segment and segment['start'] <= start <= end <= segment['end'] and quote in segment['text'])
        if sample['reference'] is None:
            continue
        reference = analysis(sample['reference']); refs += 1
        statuses += 1; correct_status += prediction['status_id'] == reference['status_id']
        a, b = set(prediction['tag_ids']), set(reference['tag_ids'])
        pred_tags += len(a); ref_tags += len(b); match_tags += len(a & b)
        a = {c['id']: c for c in prediction['criteria']}; b = {c['id']: c for c in reference['criteria']}
        pred_criteria += len(a); ref_criteria += len(b); match_criteria += len(set(a) & set(b))
        for key, expected in b.items():
            criterion_total += 1
            reference_scored += number(expected['score'])
            actual = a.get(key)
            criterion_correct += bool(actual and actual['status'] == expected['status'])
            if actual and number(actual['score']) and number(expected['score']):
                score_error += abs(actual['score'] - expected['score']); score_pairs += 1
        a, b = citations(prediction), citations(reference)
        pred_evidence += len(a); ref_evidence += len(b); match_evidence += len(a & b)
        pred_abstain += prediction['abstained']; ref_abstain += reference['abstained']
        match_abstain += prediction['abstained'] and reference['abstained']
    metrics = {'status_accuracy': ratio(correct_status, statuses), 'tag_precision': ratio(match_tags, pred_tags),
        'tag_recall': ratio(match_tags, ref_tags), 'criterion_id_precision': ratio(match_criteria, pred_criteria),
        'criterion_id_recall': ratio(match_criteria, ref_criteria), 'criterion_status_accuracy': ratio(criterion_correct, criterion_total),
        'criterion_score_mae': ratio(score_error, score_pairs) if score_pairs == reference_scored else None,
        'criterion_score_coverage': ratio(score_pairs, reference_scored), 'evidence_precision': ratio(match_evidence, pred_evidence),
        'evidence_recall': ratio(match_evidence, ref_evidence), 'evidence_validity': ratio(evidence_valid, evidence_checked),
        'abstention_precision': ratio(match_abstain, pred_abstain), 'abstention_recall': ratio(match_abstain, ref_abstain),
        'reference_coverage': ratio(refs, len(manifest['samples']))}
    counts = {'status_pairs': statuses, 'correct_status': correct_status, 'predicted_tags': pred_tags,
        'reference_tags': ref_tags, 'matched_tags': match_tags, 'predicted_criteria': pred_criteria,
        'reference_criteria': ref_criteria, 'matched_criteria': match_criteria, 'criterion_status_pairs': criterion_total,
        'correct_criterion_status': criterion_correct, 'criterion_score_pairs': score_pairs, 'reference_scored_criteria': reference_scored,
        'predicted_evidence': pred_evidence, 'reference_evidence': ref_evidence, 'matched_evidence': match_evidence,
        'evidence_checked_against_transcript': evidence_checked, 'valid_evidence': evidence_valid,
        'predicted_abstentions': pred_abstain, 'reference_abstentions': ref_abstain, 'matched_abstentions': match_abstain}
    return finish('analysis', len(manifest['samples']), refs, metrics, counts, manifest.get('thresholds', {}))


if __name__ == '__main__':
    raise SystemExit(main('analysis', METRICS, evaluate))
