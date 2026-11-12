import {test,expect,request as makeRequest,type APIRequestContext,type Page} from '@playwright/test'

async function emailedCode(mail:APIRequestContext,email:string){
 let code=''
 await expect.poll(async()=>{const list=await(await mail.get('/api/v1/messages')).json();for(const item of list.messages||[]){if(!(item.To||[]).some((to:{Address:string})=>to.Address===email))continue;const detail=await(await mail.get('/api/v1/message/'+item.ID)).json();const source=String(detail.Text||'')+' '+String(detail.HTML||'');const links=source.match(/https?:\/\/[^\s<>"']+/g)||[];for(const link of links){try{const url=new URL(link.replace(/&amp;/g,'&'));const candidate=url.searchParams.get('code');if(candidate&&/^\d{6}$/.test(candidate)){code=candidate;return true}}catch{}}const found=source.match(/(?:код|code)[^\d]{0,60}(\d{6})/i);if(found){code=found[1]!;return true}}return false},{timeout:15000}).toBe(true)
 return code
}

async function logout(page:Page){const menu=page.getByRole('button',{name:'Открыть меню',exact:true});if(await menu.isVisible()&&!await page.locator('.sidebar.open').count())await menu.click();await page.getByRole('button',{name:'Выйти',exact:true}).click()}

// This creates a real, isolated synthetic company. The operator must remove only
// the attached user/company IDs afterwards; ordinary E2E runs never opt in.
test('public registration creates an empty company, logs in and rejects duplicate registration',async({page},testInfo)=>{
 test.skip(process.env.E2E_ALLOW_REGISTRATION!=='1'||!process.env.E2E_MAILPIT,'Requires explicit permission, isolated E2E_MAILPIT and cleanup of attached synthetic IDs.');test.setTimeout(60000)
 const mail=await makeRequest.newContext({baseURL:process.env.E2E_MAILPIT});try{
 const account=`registration-e2e-${Date.now()}@example.test`,password='Synthetic-Registration!2026',company=`Регистрация E2E ${Date.now()}`
 let registrationRequests=0
 page.on('request',request=>{if(request.method()==='POST'&&new URL(request.url()).pathname==='/api/auth/register')registrationRequests++})
 await page.setViewportSize({width:1440,height:1000});await page.goto('/login');await page.getByRole('link',{name:'Зарегистрироваться',exact:true}).click();await expect(page).toHaveURL(/\/register$/);await expect(page.getByRole('heading',{name:'Создать аккаунт'})).toBeVisible()
 await page.setViewportSize({width:390,height:844});expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(390)
 await page.getByLabel('Ваше имя',{exact:true}).fill('Тест регистрации');await page.getByLabel('Название компании',{exact:true}).fill(company);await page.getByLabel('Рабочая почта',{exact:true}).fill(account);await page.getByLabel('Пароль',{exact:true}).fill(password);await page.getByLabel('Повторите пароль',{exact:true}).fill('Different-Password!2026');await page.getByRole('button',{name:'Зарегистрироваться',exact:true}).click();await expect(page.getByRole('alert')).toContainText('Пароли не совпадают');expect(registrationRequests).toBe(0)
 await page.getByLabel('Повторите пароль',{exact:true}).fill(password)
 const result=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/auth/register'&&response.request().method()==='POST');await page.getByRole('button',{name:'Зарегистрироваться',exact:true}).click();const registered=await result;expect(registered.status()).toBe(202);expect((await registered.json()).data.verification_required).toBe(true);await expect(page.getByRole('heading',{name:'Подтвердите почту'})).toBeVisible();expect((await page.request.get('/api/auth/me')).status()).toBe(401);await page.getByLabel('Код из письма',{exact:true}).fill(await emailedCode(mail,account));const verification=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/auth/verify-email'&&response.request().method()==='POST');await page.getByRole('button',{name:'Подтвердить почту',exact:true}).click();const verified=await verification;expect(verified.status()).toBe(200)
 const me=(await verified.json()).data
 await testInfo.attach('registration-cleanup-ids',{body:JSON.stringify({email:account,user_id:me.user.id,organization_id:me.organization.id}),contentType:'application/json'})
 expect(me.role).toBe('owner');expect(me.organizations).toHaveLength(1);expect(me.organization.name).toBe(company);await expect(page.getByRole('heading',{name:'Обзор звонков'})).toBeVisible()
 const calls=await page.request.get('/api/calls');expect(calls.ok()).toBe(true);expect((await calls.json()).data.total).toBe(0)
 const connections=await page.request.get('/api/connections');expect(connections.ok()).toBe(true);expect((await connections.json()).data.items).toHaveLength(0)
 await logout(page);await page.getByLabel('Рабочая почта',{exact:true}).fill(account);await page.getByLabel('Пароль',{exact:true}).fill(password);await page.getByRole('button',{name:'Войти',exact:true}).click();await expect(page.getByRole('heading',{name:'Обзор звонков'})).toBeVisible();await logout(page)
 await page.getByRole('link',{name:'Зарегистрироваться',exact:true}).click();await page.getByLabel('Ваше имя',{exact:true}).fill('Дубликат');await page.getByLabel('Название компании',{exact:true}).fill('Не должна создаться');await page.getByLabel('Рабочая почта',{exact:true}).fill(account);await page.getByLabel('Пароль',{exact:true}).fill(password);await page.getByLabel('Повторите пароль',{exact:true}).fill(password)
 const duplicate=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/auth/register'&&response.request().method()==='POST');await page.getByRole('button',{name:'Зарегистрироваться',exact:true}).click();expect((await duplicate).status()).toBe(409);await expect(page.getByRole('alert')).toBeVisible();await expect(page).toHaveURL(/\/register$/)
 }finally{await mail.dispose()}
})

test('email magic link strips its code before POST and requires the signup password in another browser',async({page,browser,baseURL},testInfo)=>{
 test.skip(process.env.E2E_ALLOW_REGISTRATION!=='1'||!process.env.E2E_MAILPIT,'Requires explicit permission, isolated E2E_MAILPIT and cleanup of attached synthetic IDs.');test.setTimeout(60000)
 const mail=await makeRequest.newContext({baseURL:process.env.E2E_MAILPIT}),other=await browser.newContext({viewport:{width:390,height:844}})
 try{
  const account=`registration-magic-${Date.now()}@example.test`,password='Synthetic-Magic-Registration!2026',company=`Регистрация по ссылке ${Date.now()}`
  await page.goto('/register');await page.getByLabel('Ваше имя',{exact:true}).fill('Тест ссылки');await page.getByLabel('Название компании',{exact:true}).fill(company);await page.getByLabel('Рабочая почта',{exact:true}).fill(account);await page.getByLabel('Пароль',{exact:true}).fill(password);await page.getByLabel('Повторите пароль',{exact:true}).fill(password)
  const sending=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/auth/register'&&response.request().method()==='POST');await page.getByRole('button',{name:'Зарегистрироваться',exact:true}).click();expect((await sending).status()).toBe(202);await expect(page.getByRole('heading',{name:'Подтвердите почту'})).toBeVisible()
  const code=await emailedCode(mail,account),recipient=await other.newPage(),posted:{url:string;hasPassword:boolean}[]=[]
  recipient.on('request',request=>{if(request.method()==='POST'&&new URL(request.url()).pathname==='/api/auth/verify-email')posted.push({url:recipient.url(),hasPassword:'password' in request.postDataJSON()})})
  const needsPassword=recipient.waitForResponse(response=>new URL(response.url()).pathname==='/api/auth/verify-email'&&response.request().method()==='POST')
  await recipient.goto(baseURL+'/verify-email?'+new URLSearchParams({email:account,code}).toString());expect((await needsPassword).status()).toBe(409);await expect(recipient.getByLabel('Пароль, указанный при регистрации',{exact:true})).toBeVisible()
  expect(posted).toHaveLength(1);expect(posted[0]!.url).not.toMatch(/[?&](email|code)=/);expect(posted[0]!.hasPassword).toBe(false);expect((await recipient.request.get('/api/auth/me')).status()).toBe(401)
  const stored=await recipient.evaluate(()=>Object.values(sessionStorage));expect(stored).toEqual([account]);expect(await recipient.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(390)
  await recipient.getByLabel('Пароль, указанный при регистрации',{exact:true}).fill(password)
  const confirmed=recipient.waitForResponse(response=>new URL(response.url()).pathname==='/api/auth/verify-email'&&response.request().method()==='POST');await recipient.getByRole('button',{name:'Подтвердить почту',exact:true}).click();const response=await confirmed;expect(response.status()).toBe(200);const me=(await response.json()).data
  await testInfo.attach('registration-cleanup-ids',{body:JSON.stringify({email:account,user_id:me.user.id,organization_id:me.organization.id}),contentType:'application/json'})
  expect(posted[1]!.hasPassword).toBe(true);expect(me.role).toBe('owner');expect(me.organizations).toHaveLength(1);await expect(recipient.getByRole('heading',{name:'Обзор звонков'})).toBeVisible();expect(await recipient.evaluate(()=>Object.values(sessionStorage))).toEqual([]);expect((await(await recipient.request.get('/api/calls')).json()).data.total).toBe(0);await logout(recipient)
 }finally{await other.close().catch(()=>{});await mail.dispose()}
})
