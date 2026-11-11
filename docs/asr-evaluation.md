# Offline оценка расшифровок

`scripts/eval-transcripts.py` сравнивает заранее сохранённые predictions с независимыми references. Скрипт не вызывает ASR или сеть. Реального размеченного корпуса в проекте пока нет; синтетические unit-тесты проверяют математику/валидацию, а не качество распознавания.

```sh
python3 scripts/eval-transcripts.py --manifest /private/asr-manifest.json \
  --report data/eval/asr-quality.json
```

Manifest — обычный файл `0600`, без symlink, максимум 32 MiB / 1000 samples. Текст есть только во входе; stdout и report содержат агрегаты, без текста, телефонов, sample IDs и путей входных файлов. Report создаётся с правами `0600`. Входные записи не копируются в Git.

Формат (вымышленный пример, не готовый benchmark):

```json
{
  "version": 1,
  "task": "transcripts",
  "thresholds": {
    "wer": {"max": 0.2},
    "role_accuracy": {"min": 0.9},
    "timestamp_mae_seconds": {"max": 0.5}
  },
  "samples": [{
    "reference": {
      "text": "Добрый день",
      "segments": [{"start": 0, "end": 2, "role": "manager"}]
    },
    "prediction": {
      "text": "добрый день",
      "segments": [{"start": 0.1, "end": 2.1, "role": "manager"}]
    }
  }]
}
```

`reference` может быть `null`: sample учитывается как missing reference, качество не считается подтверждённым. Reference/prediction имеют обязательный `text` (до 20000 символов), необязательные `segments` со start/end в секундах и role `manager|client|unknown`. Границы конечны, неотрицательны, упорядочены по start, end≥start. Для слова — максимум 5000 tokens. Extra keys и неверные типы отклоняются; bool не считается числом.

WER = сумма точных Levenshtein word edits / сумма reference words. Word tokenizer: Unicode `\w+`, NFKC и casefold; пунктуация не входит в слова. CER использует тот же NFKC/casefold, удаляет whitespace, сохраняет пунктуацию. Публикуются edit counts и denominator; при нулевом denominator метрика `null`, а не искусственный0/1. Короткие/длинные samples агрегируются по числу reference units, не средним от sample WER.

Timing MAE сравнивает start/end по порядку **только при одинаковом количестве segments**. При другом разбиении alignment не выдумывается: mismatch count растёт, coverage уменьшается, общая timing/role метрика остаётся `null`, если не весь reference scope сопоставим. Это не алгоритм автоматического alignment. Role accuracy сравнивает только известные reference роли; predicted unknown на таком сегменте — ошибка. Unknown reference роли не объявляются правильными. Coverage и число timing boundaries/role segments доступны отдельно.

Thresholds необязательны, имена берутся из метрик harness; каждый порог имеет ровно `min` или `max`. `acceptance_passed=null` означает отсутствие thresholds, reference или вычислимой обязательной метрики. Неудачный измеренный порог даёт `false`; все заданные пороги и полное reference покрытие дают `true` только для предоставленного corpus. Exit0 — валидный отчёт (включая unknown), exit2 — измеренный threshold failure, exit1 — неверный manifest/report. Примерные числа выше не являются согласованными нормативами продукта.

Перед live acceptance нужно независимо разметить разрешённые русские звонки с каналами/ролями, тишиной, шумом, переводами и длинными записями; разделить tuning/held-out наборы; согласовать thresholds с владельцем и сохранить provenance локально. Until then реальное качество ASR остаётся неизвестным. `python3 -m unittest discover -s scripts/tests -p test_evaluation.py -v` проверяет harness на синтетических случаях.
