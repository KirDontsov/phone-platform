---
phase: 17-telphin-empty-history-date
plan: 01
subsystem: telphin-sync
---
# Phase17 — исправление импорта истории

Причина подтверждена на реальном подключении: `history_start` сохранена пустой строкой, необязательные payload from/to отсутствуют, до sync_run дело не доходило. Настоящий Telphin API возвращает корректные UTC timestamps; provider parser не изменён.

Worker использует sync_range с trim и пропуском пустых optional dates. Сохраняются defaults 30/364 дня, строгие непустые даты и checkpoints, UTC и maximum3650days. Нижняя граница не становится подтверждённой из-за fallback. Успешный retry возвращает только error→configured, чтобы scheduler продолжал polling; disconnected не активируется.

Независимые PLAN-CHECK/revision0 и implementation review PASS. Пять новых range regressions; RED без фильтрации: две исходные ошибки воспроизводятся, три контрольных сценария проходят. GREEN workerlib/providers/modelHTTP: 42 фактически выполнены, один существующий PG-case пропущен без TEST_DATABASE_URL (Cargo формально отображает43ok). Для recovery-state отдельный RED не делался. Backend schema/frontend contract не менялись; мелкие изменения UI не требовались.

Code2087a86 опубликован в GitHub и собран nativeamd64, runtime15-2087a86 активирован на phone.bondscan.ru:8healthy. Предварительный b238453 не активировался. DB/ключи/Codexauth/чужие приложения не восстанавливались/не ротировались. Только исходный failed sync одной компании атомарно поставлен снова по guards org/job/subject/kind/state/error; receipt retried_count1. Не создавались другие importjobs/outbox duplicates, PostgreSQLpolling использует тот же job. Пользовательские credentials не менялись.

Postdeploy HTTPS smoke PASS: owner/manager/reviewer/securedcookie/tenantguards/mediaRange/Codexauth; local analyzer doctor реальный roundtrip/pairing healthy. Все прочие provider/model запросы smoke равны0. Возобновлённый импорт выполняет разрешённые пользователем настоящие чтения Telphin, анализ/ASR не запускались оператором. Live-final и независимый verifier фиксируются в17-VERIFICATION.md; human UAT в17-UAT.md pending.

Ограничение: это успешная загрузка предоставляемого API в существующем окне, а не доказательство полной исторической базы или качества ASR. Phase12 paused/stash сохранён. Чеклист Григория отсутствует.

Live:43370уникальныхзвонков; checkpointwindow33/page7. Telphin429 запросилчасовуюпаузу, jobretry/attempt1 сnext_attempt_at. Независимыйverifier PASSнаimplementation/release/liveprogress/backoff, finalcompletion/configured/pollingpending. Не обходитьквотуи не повторятьимпорт вручную.
