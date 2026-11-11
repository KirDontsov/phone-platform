<script setup lang="ts">
defineProps<{message?:string;kind?:'error'|'success'|'info'}>();
</script>
<template><div v-if="message" class="notice" :class="kind||'info'" :role="kind==='error'?'alert':'status'"><slot>{{message}}</slot></div></template>
