<script setup lang="ts">
import type {Call} from '../../shared/interfaces'
import {analysisLabel,isActive,processingLabels,stepLabels} from '../../shared/lib/processing'
import Icon from '../../shared/ui/Icon.vue'
defineProps<{call:Call}>()
</script>
<template><div class="analysis-cell"><span class="badge" :class="call.analysis_state==='analyzed'?'teal':call.analysis_state==='partial'?'amber':'outline'"><Icon :name="call.analysis_state==='analyzed'?'check':call.analysis_state==='partial'?'alert':'activity'" :size="11"/>{{analysisLabel(call)}}</span><small v-if="isActive(call.processing)">{{stepLabels[call.processing!.step||'']||'Обработка'}} · {{processingLabels[call.processing!.state]}}</small><small v-else-if="call.processing?.state==='failed'" class="processing-error">Ошибка обработки</small><small v-else-if="call.transcript_state==='ready'&&call.analysis_state==='not_analyzed'">Текст готов</small></div></template>
