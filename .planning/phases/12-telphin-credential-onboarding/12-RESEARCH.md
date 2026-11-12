# Phase12 research

Пользовательский feedback: API-only onboarding требует неизвестных владельцу полей. Имеются обычные логины/пароли кабинета и АТС. Подтверждён официальный bootstrap: PBX login→app/register→trusted CallAPI→AppID/Secret, client_id из /user/. Анонимная server-rendered login form username/password/_csrf_token исследована; behindauth registration markup/secretredisplay пока неизвестны. Детали/источники/границы: ../../research/TELPHIN-PROVISIONING.md.

UX draft: docs/assistant-ui.md. Контракт: docs/contracts/telphin-onboarding-v1.md. Выбран детерминированный fixed backend HTTP adapter, не generalbrowserMCP и не передача паролей LLM. Девятый fixed provision tool, durable external-effect ledger и failclosedmanualreconciliation после ambiguousPOST. Существующая APIформа остаётся advanced. Billing и PBX пары не смешиваются.
