<script setup lang="ts">
import Icon from './Icon.vue';withDefaults(defineProps<{title?:string;description?:string;loading?:boolean}>(),{title:'Пока ничего нет',description:'Данные появятся после подключения источника.'});
</script>
<template><div class="empty"><div class="empty-icon" :class="{spin:loading}"><Icon :name="loading?'refresh':'phone'" :size="26"/></div><h3>{{loading?'Загружаем данные':title}}</h3><p>{{loading?'Это займёт несколько секунд.':description}}</p><slot/></div></template>
