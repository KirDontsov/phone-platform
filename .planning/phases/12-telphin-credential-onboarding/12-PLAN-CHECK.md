---
phase: "12-telphin-credential-onboarding"
status: PASS
checked_at: "2026-11-12"
revision: 1
revision_cap: 3
execute_gate: allowed
---

# Независимая проверка плана фазы12

Проверены research, [контракт](../../../docs/contracts/telphin-onboarding-v1.md), оба плана и фактический GSD structure verifier. Reviewer не автор плана/API/portal adapter/frontend. Это проверка плана; implementation VERIFICATION отсутствует и PASS реализации не заявляется.

## Результат

**PASS после revision1/3; execute gate разрешён.** Открытых P1/P2 в проверенном плане нет. Первоначальный P1 закрыт изменениями контракта и12-01, перечисленными ниже. Повторные фактические structure checks двух планов: `valid=true`, errors0/warnings0; все5tasks имеют files/action/verify/done. Секреты через scoped refs, fixed official transport, CSRF/cookie isolation, typed needs_action при MFA/unknown forms, raw provider errors redaction, existing connection/CAS, no blind retry/no delete и отдельный pending live gate заданы корректно.

### Первоначальный P1-01 — account-keyed reservation не задавала единую границу создания приложения

Где: контракт «Authority, CAS и side effects», ledger `stable org+host+login fingerprint`, `lease_until` и «Pending lease блокирует параллельный account attempt»; план12-01 task2. Текущая формулировка оставляет три опасных неоднозначности:

1. У компании одно локальное подключение Телфина, но разные PBX refs/login aliases/host hints могут получить разные account ledger keys. Два параллельных вызова до создания local connection проходят initial empty-connection check и способны отправить два registration POST. Поздний CAS запретит второй local commit, но второе удалённое приложение уже существует.
2. `lease_until` без явно обязательного reservation owner/generation и conditional before-POST checkpoint не определяет, как запретить старому ожидающему HTTP attempt создание после takeover expired lease.
3. Snapshot может быть снят до `create_attempted`, а внешний POST выполниться после snapshot. Restore помечает unfinished ledger uncertain, но необходимо явно запретить новую регистрацию и тогда, когда сохранённый create_attempted=false. Отсутствие флага в старом snapshot не доказывает отсутствие внешнего эффекта.

Требуемая доработка: зафиксировать org-wide singleton provisioning gate, охватывающий все PBX refs/logins/hosts до существования локального подключения, и монотонную state machine с reservation owner/generation. Каждая before-stage проверка и атомарный переход перед единственным registration POST должны проверять актуальные owner/fence/authority и отсутствие ранее attempted/uncertain effect. Restore любой unfinished reservation запрещает новый POST независимо от snapshot-флага; продолжение — только read/reconciliation/manual action. Существующие безопасные account marker/fingerprint и provider app checks сохраняются, generic tools не нужны.

Приёмка revision: контракт/план требуют реальные PG/Driver barriers для двух разных PBX refs в одной org, expired reservation с поздним старым guard и restore snapshot-before-POST. Общее число registration POST не больше1; stale execution не получает права на create/adopt; неопределённый результат не превращается в новую регистрацию. Ключи/таймеры/UI не расширяют authority.

## Повторная проверка revision1

Прочитаны изменённые контракт и12-01; решения P1 сформулированы явно:

- Singleton `telphin_provision_gate` имеет stable org-only ID и охватывает все refs/logins/hosts, независимо от account fingerprint.
- Reservation выдаёт monotonic fence; каждый guard и conditional beforePOST проверяют exact attempt/fence/lease и original authority под org lock. Старый execution после takeover не разрешает POST.
- Create-attempted/uncertain/creation-forbidden не снимаются истечением lease; известный отказ до create освобождает только своё exact fence.
- Restore **любого** unfinished gate/ledger выставляет creation_forbidden=true и повышает fence, включая snapshot до create_attempted. Read/reconciliation не превращается в новый POST на основании старого false-флага.
- План требует реальные PG/Driver concurrent differentrefs/hosts, expired-fence staleguard и restore-before-POST tests, помимо прежних replay/CAS/cancel/ambiguous cases.

**P1-01 closed at plan level.** Эти условия подлежат проверке на реализации; наличие их в тексте не является выполненным regression или внешней регистрацией. Следующий шаг — execute двух планов, затем independent implementation verifier и automated UAT. Реальная behind-auth HTML схема и полномочия конкретного аккаунта остаются pending live acceptance; synthetic fixtures не закрывают этот gate.

Проверенные команды:

```sh
node /Users/kirilldoncov/.claude/get-shit-done/bin/gsd-tools.cjs verify plan-structure .planning/phases/12-telphin-credential-onboarding/12-01-PLAN.md
node /Users/kirilldoncov/.claude/get-shit-done/bin/gsd-tools.cjs verify plan-structure .planning/phases/12-telphin-credential-onboarding/12-02-PLAN.md
```
