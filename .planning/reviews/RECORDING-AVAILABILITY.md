# Записи: первичный импорт и лимит Телфина

Пользователь сообщил «записи недоступны» после подключения настоящего Телфина. Проверка только runtime metadata/состояния очереди, без содержимого разговоров, телефонов, токенов или provider URLs в обычных логах.

Причина: первоначальная загрузка успела скачать391реальный файл,126jobs перешли в retry/provider_rate_limited (429), next_attempt_at=2026-11-12T13:00Z. В компании также17syntheticreadyfiles, поэтому общий ready408 не равен количеству реальных файлов. Ошибок permissions/формата в этой очереди на момент проверки нет. Worker healthy, повтор планируется штатным механизмом; ограничения провайдера не обходились и очередь вручную не менялась.

UIдефект: AudioPlayer открывал media URL даже дляready=false; backend закономерно отвечал404 recording_unavailable, а браузер показывал общую ошибку и не перезагружал тот жеsrc послеpolling. Теперь pending/rate_limited/failed/ready разделены, src создаётся только дляready, послеобновленияданныхплеер появляетсясам. Предпочтение готовому файлу, выбранныйID/DOMaudio сохраняются приpolling/reorder; ручнойвыборуважается. APItenant guard не менялся.

Проверки:51/51frontendPASS, typecheck/Vite/DockerbuildPASS;5новыхbehaviourregressions падают на прежнемкомпоненте, guardочисткиошибкиприcall switch уже проходил на baseline. Actualready media HTTP206/audio-mpeg, browserreadyState4/errornull; mutedplayback currentTime advancing подтвердилвоспроизведение безпрослушивания. Actualpendingcall черезреальный API показывает providerlimitnotice и НЕсоздаёт audioelement. Скриншоты/аудио/текстразговоров не сохранялись. Aggregateprivate JSON: data/eval/recording-player-live-smoke.json.

Client пересобран/развёрнут. Внешний лимит остаётся открытым operationalcondition; запланированныйretry не является доказательством будущего provider success. Полнотаистории/качествоASR и общаяприёмкаТелфина этимне закрываются.
