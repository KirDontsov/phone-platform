# Фаза 11 — независимый implementation review и закрытие замечаний

Дата: 2026-11-12. Статус: **local verification PASS; human/external acceptance pending**. Проверяющий не автор core API/runner/UI; автор acceptance tests. Область: server authority/tenant/secret isolation, lifecycle/receipt/CAS алгоритмы, lock races, restore, границы pinned Codex executor, согласованность публичных и private contracts. Полные результаты и provenance: [11-VERIFICATION](../phases/11-configuration-assistant/11-VERIFICATION.md).

| Приоритет | Дефект, обнаруженный во время execute/review | Закрытие |
| --- | --- | --- |
| P1 | Live session/auth generation проверялись до lock/async preflight; отзыв власти мог проиграть ожидающему tool или позднему auth callback | Org→conversation→run locks и live rechecks, исходная session/membership/generation; actualPG race/auth regressions PASS |
| P1 | FK KEY SHARE и несовместимый org FOR UPDATE вместе с conversation-first cancel создавали настоящий deadlock | Org FOR NO KEY UPDATE и единый org-first order; двусторонний concurrent regression PASS |
| P2 | Stalled stream не сохранял ранний single partial; EOF/malformed могли оставить неверное представление прогресса | Timer-driven flush, bounded strict parser/terminal handling; real HTTP stream regression PASS |
| P2 | Schema failures обходили quota/receipt; replay invalid args мог менять результат. Nested calendar/rubric validator отличался от SDK | Savepoint сохраняет failed receipt/budget/event вне отката semantic mutation; strict nested parity;23failures+limit24/replay regression PASS |
| P2 | No-op lookup обходил stale rowversion; общий settings snapshot не имел общего bytecap | Rowversion/CAS перед no-op,128KiB explicit413; actualPG scope PASS |
| P2 | Последний независимый срез: `.take(100)` скрывал101-й compact setting, не достигающий bytecap | Count>100 теперь explicit413 assistant_context_too_large; финальный targeted scope1/1 PASS2.94s проверяет101statuses и точный replay safe error |
| P2 | Timestamp-tied messages менялись местами; terminal metadata терялись; completed progress statuses становились uncertain | Persisted sequence, bounded model/usage projection, completed-status projection; API assertions и fresh browser5eventscompleted PASS |
| P2 | Restore мог вернуть старую authority/temporary credential material или удалённый chat subtree | Actual PG deletion-ledger/revoke regressions,17/17 backup suite и actual isolated restore с22media PASS |

Дополнительный независимый относительно root API source-review исполнителя runner: [11-RUNNER-API-REVIEW](11-RUNNER-API-REVIEW.md). Его первоначальные замечания, включая nested schema parity, source-resolved; окончательные assertions перечислены выше. Review не исчерпывает все возможные состояния и не заявляет отсутствие неизвестных дефектов.

Новых незакрытых P1/P2 в проверенном scope не выявлено. Ограничения продукта видимы: workspace свыше100objects в одном типе или128KiB завершается явной ошибкой; длительность и tool budget ограничены; cancel сохраняет уже committed действия; без auth нет fake ответа. Модель не получает OS/repository/DB/socket и arbitrary HTTP/SQL tools. Реальный pinned executor negative probe и SDK→API call пройдены с offline Responses fixture, а не с платной моделью. Анализатор звонков остался отдельным контуром с MCP0.

Для финальной приёмки остаются: вход владельца в настоящий Codex, model entitlement/inference и корректность смысловой настройки, live Телфин/test/import и человеческая оценка UX. Их нельзя закрыть синтетическими fixture reports. Финальный сценарий: [11-UAT](../phases/11-configuration-assistant/11-UAT.md).
