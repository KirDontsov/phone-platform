# Worker/API execute14

Provideropencode ASR использует отдельный typed STT adapter module, exact model allowlist, operator-owned MODEL_RUNNER_URL и runtime transport password. API-key config принадлежит компании/OpenRouter и передаётся отдельно от Basicauth. Текстовый анализ через native OpenCode и прежняя авторизация сохраняются. /connections/asr/test проверяет настоящую provider auth без аудио; mock upstream отдельно маркирован.

Preflight и server config validation поддерживают новую схему. Смена ASR provider/baseURL не наследует прежний ключ при пустом/sentinel поле; same-provider secret сохраняется. Изолированный PostgreSQL/Actix test доказываетHTTPsave/redaction/rollback/tenantboundaries/models/URL/preflight. Invalidmodels инесовпадающийURL отклоняются до чтения файла/сети.

Длинная запись делится на300s/24MiB части, timestampsсдвигаются, speakerlabels0/1сохраняютсястроками иразделяютсямеждучастями, ролиunknown; knownusage сохраняется, missingcost не превращается в0. Бинарный ffmpeg/config никак не задаётся пользователем модели; native/MCPtool policy не менялась. RAIIдиректория защищает временныеchunks уже во время cancellable ffmpeg/read_dir, а не только после возвратаAudioChunk.

Проверки: model_http8/8 иproviders16/16 PASS; actualAPI/PGnew1 иaffectedprocessing/recording2PASS. Finalworkerlib14/model_http8/providers16/pipelinePG7 consolidatedPASS (data/eval/phase14-worker-final.log). ASR protocol baselineRED отдельно, URL/modelearlyguardRED; APIproviderkeymutationRED и cancellationRAIImutationRED в sourcecopy по assertions. Для остальных новых guard cases отдельныйRED не проверялся. Изолированные synthetic fixtures, реальных model/ASR/Telphin calls0.

Во время sourcecopy RED совместный Cargo target однажды использовал mutation test artifact; оригинал принудительно пересобран, после этого весь итоговый affected worker suite PASS. Основной source не мутировался. Приватные отчёты data/eval/phase14-api-final.log,phase14-api-key-guard-red.log,phase14-worker-protocol-red.log,phase14-cancellation-red.log. Итоговая отдельная testDB удаляется root после verifier.

Основной platformAPI/worker/client/modelrunnerDockerbuilt/uphealthy; standalone localanalyzer также поднят на этом компьютере. Реальные SSHhost/pairing/OpenRouterkey/качество pending и не заменяются fixtures.
