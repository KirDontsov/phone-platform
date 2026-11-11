# Offline оценка анализа диалогов

`scripts/eval-analysis.py` сравнивает уже полученный анализ с независимой разметкой руководителя. Модель и сеть не вызываются. Реальный reference corpus и согласованные thresholds пока отсутствуют: наличие harness не подтверждает качество AI.

```sh
python3 scripts/eval-analysis.py --manifest /private/analysis-manifest.json \
  --report data/eval/analysis-quality.json
```

Private manifest/report имеют те же ограничения, что [ASR harness](asr-evaluation.md): ordinary file `0600`, no symlink, ≤32 MiB / 1000 samples; stdout/report — только агрегаты, без текста/цитат/IDs/телефонов.

Формат — нормализованная проекция результата и reference. Поле модели `suggested_status_id` переносится в `status_id`, `suggested_tag_ids` в `tag_ids`, явное наличие обоснованного отказа — в `abstained`. Summary/intent/usage и другие поля не передаются в этот mini schema. Пример вымышленный:

```json
{
  "version": 1,
  "task": "analysis",
  "thresholds": {"status_accuracy": {"min": 0.9}, "evidence_validity": {"min": 1}},
  "samples": [{
    "segments": [{"id": "s1", "start": 0, "end": 2, "text": "Здравствуйте"}],
    "reference": {
      "status_id": "consultation", "tag_ids": ["greeting"], "abstained": false,
      "criteria": [{"id": "hello", "status": "scored", "score": 4,
        "evidence": [{"segment_id": "s1", "start": 0, "end": 2, "quote": "Здравствуйте"}]}]
    },
    "prediction": {
      "status_id": "consultation", "tag_ids": ["greeting"], "abstained": false,
      "criteria": [{"id": "hello", "status": "scored", "score": 4,
        "evidence": [{"segment_id": "s1", "start": 0, "end": 2, "quote": "Здравствуйте"}]}]
    }
  }]
}
```

`reference:null` означает отсутствие разметки; `acceptance_passed` остаётся `null`. Необязательные segments позволяют проверить фактические citations без оценки руководителя, но не превращают такой sample в принятую quality-reference. Критерии уникальны; scored требует конечный score0…5, not_applicable/insufficient_evidence — score null. Tags≤30 и unique, status nullable, abstained — bool. Evidence требует segment_id, start/end, непустую quote; неизвестные keys отклоняются.

Метрики:

- Status accuracy — точное совпадение с reference, включая осмысленный null.
- Tags и criterion IDs — micro precision/recall по множествам. Criterion status accuracy считает отсутствующий reference criterion ошибкой. Score MAE сравнивает сопоставимые numeric scores; при отсутствующих predicted scores становится `null`. Score coverage показывает долю reference scored criteria с numeric prediction.
- Evidence precision/recall — точное совпадение `(criterion_id,segment_id,start,end,quote)` с reference. Другая допустимая цитата может не совпасть с human annotation; это alignment metric, не самостоятельное доказательство плохого анализа.
- Evidence validity — существующий segment, точная substring quote, границы внутри segment. Это подтверждает только опору на предоставленный текст, а не смысловую правильность вывода.
- Abstention precision/recall — micro TP/predicted/expected по явному `abstained`. При отсутствии положительных examples denominator0 даёт `null`, не100%.

Публикуются знаменатели/sample counts; отсутствующая reference/невычислимая метрика/неуказанные thresholds не дают `passed=true`. При хотя бы одном измеренном провале — `false`; exit2. Unknown — exit0 с `acceptance_passed:null`; malformed input — exit1 с безопасным кодом ошибки. Numeric output не заменяет проверку смысла резюме, согласий и next steps: для этого нужна независимая reference methodology и аудит руководителя.

Для приёмки нужны разрешённый held-out corpus, отдельные ошибки primary/repeat/unknown и manual overrides, роли/доказательства, prompt injection, неполная история и согласованные thresholds. Фактическое качество модели, Codex auth, цена/usage и budget остаются отдельными live gates. Синтетические unit-тесты harness: `python3 -m unittest discover -s scripts/tests -p test_evaluation.py -v`.
