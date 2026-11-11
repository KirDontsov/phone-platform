#!/usr/bin/env python3
"""Offline transcript WER/CER/timing/role aggregates; no ASR calls or text in reports."""
import re
from evaluation import edit_distance, finish, main, normalized, number, object_keys, ratio, require, text

METRICS = ('wer', 'cer', 'timestamp_mae_seconds', 'role_accuracy', 'timestamp_coverage', 'role_coverage', 'reference_coverage')


def transcript(value):
    object_keys(value, ('text',), ('segments',))
    text(value['text'])
    segments = value.get('segments')
    if segments is not None:
        require(isinstance(segments, list) and len(segments) <= 10000)
        previous = -1
        for segment in segments:
            object_keys(segment, ('start', 'end'), ('role',))
            start, end = segment['start'], segment['end']
            require(number(start) and number(end) and 0 <= start <= end <= 86400 and start >= previous)
            previous = start
            if 'role' in segment:
                require(segment['role'] in ('manager', 'client', 'unknown'))
    return value


def evaluate(manifest):
    words = word_edits = characters = character_edits = refs = role_total = role_correct = boundaries = 0
    boundary_error = 0.0
    timing_samples = role_samples = segmentation_mismatches = 0
    timing_reference_samples = role_reference_segments = 0
    for sample in manifest['samples']:
        object_keys(sample, ('prediction', 'reference'))
        prediction = transcript(sample['prediction'])
        if sample['reference'] is None:
            continue
        reference = transcript(sample['reference']); refs += 1
        a = re.findall(r'\w+', normalized(reference['text']))
        b = re.findall(r'\w+', normalized(prediction['text']))
        require(len(a) <= 5000 and len(b) <= 5000)
        words += len(a); word_edits += edit_distance(a, b)
        a = ''.join(normalized(reference['text']).split())
        b = ''.join(normalized(prediction['text']).split())
        characters += len(a); character_edits += edit_distance(a, b)
        expected, actual = reference.get('segments'), prediction.get('segments')
        if expected:
            timing_reference_samples += 1
            role_reference_segments += sum(s.get('role') in ('manager', 'client') for s in expected)
        if expected is None or actual is None:
            continue
        if len(expected) != len(actual):
            segmentation_mismatches += 1
            continue  # No fabricated index alignment when segmentation differs.
        if expected:
            timing_samples += 1
        roles_before = role_total
        for reference_segment, prediction_segment in zip(expected, actual):
            for key in ('start', 'end'):
                boundary_error += abs(reference_segment[key] - prediction_segment[key]); boundaries += 1
            if reference_segment.get('role') in ('manager', 'client'):
                role_total += 1
                role_correct += reference_segment['role'] == prediction_segment.get('role')
        role_samples += role_total > roles_before
    metrics = {'wer': ratio(word_edits, words), 'cer': ratio(character_edits, characters),
               'timestamp_mae_seconds': ratio(boundary_error, boundaries) if timing_samples == timing_reference_samples else None,
               'role_accuracy': ratio(role_correct, role_total) if role_total == role_reference_segments else None,
               'timestamp_coverage': ratio(timing_samples, timing_reference_samples),
               'role_coverage': ratio(role_total, role_reference_segments), 'reference_coverage': ratio(refs, len(manifest['samples']))}
    counts = {'reference_words': words, 'word_edits': word_edits, 'reference_characters': characters,
              'character_edits': character_edits, 'timestamp_boundaries': boundaries,
              'timestamp_samples': timing_samples, 'timestamp_reference_samples': timing_reference_samples,
              'role_reference_segments': role_reference_segments, 'role_segments': role_total, 'role_correct': role_correct,
              'role_samples': role_samples, 'segmentation_mismatch_samples': segmentation_mismatches}
    return finish('transcripts', len(manifest['samples']), refs, metrics, counts, manifest.get('thresholds', {}))


if __name__ == '__main__':
    raise SystemExit(main('transcripts', METRICS, evaluate))
