# Client ID: исправление пользовательского блокера

Пользователь получил App ID/App Secret, но сохранить доступы было нельзя из-за обязательного неизвестного ему числового Client ID. Узкий фикс фазы11/SETUP-01; фаза12 по логину/паролю остаётся на паузе и её draft не опубликован.

Research: существующий официально реализованный client_credentials→/api/ver1.0/user/ уже возвращает client_id. План: сделать его необязательным на вводе, определить на сервере, сохранить только проверенный ID в encrypted config; explicit IDs сохранять; ошибки/scope/revoke не превращать в успешное сохранение. Никаких новых моделей/MCPtools/HTMLавтоматизаций.

Реализация: phone-worker providers::resolve_telphin_config и тот же authenticated scope guard; API credential/saveconnection resolver внеDBtransaction с25s timeout, officialhostallowlist, safe errors и original session recheck; при updateconnection также rowversion CAS послеHTTP. UI/JSON принимают два реквизита, показывают automatic ID и безопасные ошибки.

Independent verifier fix_cli_review: realHTTP/PG4/4 PASS,5s, maskedsecretmerge/privacy/foreignscope/logoutawait/concurrentCAS. Provider implementation/tests domain_persistence: worker32/32PASS (13lib+14providers+5modelHTTP), включая5новых identity regressions среальнымloopback grant/user. Frontend45/45PASS, typecheck/Vite/DockerbuildPASS.

REDпроверки: при возврате старого mandatory-ID branch provider missing/blank test падает invalid_client_id; при возврате прежних APIroutes positive save test падает400 в обоихпутях;2newfrontendtests падают на исходныхform/parser; actualbrowser1/1 сначала RED на старомdeployedrequiredattribute, затем GREEN послеDockerdeploy (1.4s). Отдельные REDмутации securitynegative assertions не выполнялись.

API/worker/client пересобраны и запущены из отдельного cleanworktree, чтобы не публиковать pausedphase12draft. Native Codex/auth/runtime неизменны, вход пользователя сохранён. Реальный Телфин с ключами пользователя ещё не вызывался: дляfinalcheck надо обновить страницу и сохранить App ID/App Secret с пустым Client ID.
