import {createRouter,createWebHistory} from 'vue-router'
import {useAuth} from './entities/auth/model'
const router=createRouter({history:createWebHistory(),routes:[
{path:'/login',component:()=>import('./pages/auth/AuthPage.vue'),meta:{public:true}},
{path:'/register',component:()=>import('./pages/auth/AuthPage.vue'),meta:{public:true}},
{path:'/verify-email',component:()=>import('./pages/auth/VerifyEmailPage.vue'),meta:{public:true}},
{path:'/reset',component:()=>import('./pages/auth/AuthPage.vue'),meta:{public:true}},
{path:'/invite/accept',component:()=>import('./pages/auth/AuthPage.vue'),meta:{public:true}},
{path:'/',redirect:'/dashboard'},
{path:'/dashboard',component:()=>import('./pages/dashboard/DashboardPage.vue')},
{path:'/calls',component:()=>import('./pages/calls/CallsPage.vue')},
{path:'/calls/:id',component:()=>import('./pages/call-detail/CallDetailPage.vue')},
{path:'/callbacks',component:()=>import('./pages/callbacks/CallbacksPage.vue')},
{path:'/settings/:tab?',component:()=>import('./pages/settings/SettingsPage.vue')},
{path:'/integrations',component:()=>import('./pages/integrations/IntegrationsPage.vue')},
{path:'/assistant',component:()=>import('./pages/assistant/AssistantPage.vue')},
{path:'/operations',component:()=>import('./pages/operations/OperationsPage.vue')},
{path:'/:pathMatch(.*)*',redirect:'/dashboard'}]})
router.beforeEach(async to=>{const auth=useAuth();if(!auth.ready)await auth.load();if(!to.meta.public&&!auth.me)return{path:'/login',query:{redirect:to.fullPath}};if(['/login','/register'].includes(to.path)&&auth.me)return'/dashboard'})
export default router
