# RouterAI: уточнение пользователя 2026-11-13

Пользователь явно выбрал RouterAI. На этом компьютере глобальный OpenCode config содержит routerai/baseURL/6textmodels, но inlinekey/ROUTERAI_API_KEY/legacyglobalauth/SQLitecredential отсутствуют (opencode2authlistnone; credentialtableempty). Не читатьBondscanсекреты илиперебиратьдругиеприложения. НельзяподставлятьAPInexиливыдумыватьдоступ.

Реальный public RouterAI GET /api/v1/models200:606моделей, присутствуют точные moondream/parakeet-ultra и nvidia/nemotron-3.5-transcribe, modalityaudio→transcription. Первичная STTдокументация подтверждает JSONinput_audio иverbose_json через https://routerai.ru/api/v1/audio/transcriptions. usage.costвРУБЛЯХ, неUSD; сохранятьcurrency/неподмешиватькнеизвестнойстоимости.

ПервичнаяOAuthPKCEдокументация: https://routerai.ru/docs/guides/overview/oauth-pkce . /auth?code_challenge=S256&callback_url=http://127.0.0.1:<port>/callback&state&app_name. Пользовательсамподтверждаетсуществующийаккаунт. POST/api/v1/auth/keys возвращаетключ, безрегистрацииприложения иручногокопирования. Одноразовыйcode5min; повторexchangeтого-жекодавернымverifierотзываетвыданныйключ, поэтомуНЕretryнаtimeout. Приложениеидентифицируетсяapp_nameдляlocalhost, использоватьсобственноеPhoneимя+org, неOpenCode/Bondscan. Токеныдругихприложенийнеперевыпускать. СекретыниURLналогининеобычномвыводе.

Минимальныйscope: добавитьRouterAIJSONSTTвсуществующийgateway/localcatalog/workerиUIcatalog, tenantbindingне меняется; импортироватьвыбранныйRouterAI изсуществующегоглобальногоinline/legacyauthеслиесть, иначеone-timehostOAuthhelperсprivatePhoneключом. Никакойсменыключейдругихприложений/покупок/автоматическогоанализааудио. Доступсаккаунта/humanconsent остаютсячестнымexternalgate, независимыйкод/протоколпроверитьлокальнымиHTTPfixtures.
