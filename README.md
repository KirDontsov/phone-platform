# Phone Platform

Платформа анализа звонков Телфин для нескольких независимых компаний с изолированными данными. За основу стека и устройства фонового анализа взят Bondscan из `../obligation-platform`.

Реализован первый CLI-прототип: OAuth/probe, выгрузка истории по страницам, checkpoint/resume, изоляция каталогов компаний и отчёт сверки. 50 offline-тестов проходят. По указанию пользователя живое подключение пока отложено; реальные звонки не загружались. Веб-платформа и AI-анализ идут следующими фазами.

```bash
python3 scripts/telphin-spike.py --help
python3 -m unittest discover -s scripts/tests -p 'test_telphin_spike.py'
```

[Инструкция CLI и локальная настройка](docs/telphin-spike.md), [контракт API и live-неизвестные](docs/provider-contract.md).

- [Контекст проекта](.planning/PROJECT.md)
- [Требования и покрытие фазами](.planning/REQUIREMENTS.md)
- [Дорожная карта](.planning/ROADMAP.md)
- [Архитектура](.planning/ARCHITECTURE.md)
- [Исследование](.planning/research/SUMMARY.md)
- [GSD-процесс и revision-loop](.planning/WORKFLOW.md)
- [Независимая проверка планов — passed](.planning/PLAN-CHECK.md)
- [Текущее состояние](.planning/STATE.md)

Планирование завершено: 10 фаз, 26 планов реализации, 38 требований. Пройден независимый plan-checker и два цикла доработки; все требования покрыты. Проверки реального API и модели запланированы отдельно.

CLI обходит историю без исключения пропущенных и звонков без записи. Для явного периода различает законченные и незавершённые окна; в режиме `--all-available` ограничивает бюджет поиска и честно показывает неизвестную нижнюю границу. Полнота реальной истории подтверждается отдельно в 01-02.

План текущей работы: [01-01 — контракт OAuth и CLI выгрузки](.planning/phases/01-telphin-api-spike/01-01-PLAN.md). Для 01-02 уже реализованы failure fixtures и offline `reconcile`; live-сверка и UAT остаются впереди.
