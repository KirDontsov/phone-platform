<script setup lang="ts">
import {computed} from 'vue'
import type {Call} from '../../shared/interfaces'
import {recordingState,recordingLabels,recordingPresentation} from '../../shared/lib/recording-state'
import Icon from '../../shared/ui/Icon.vue'
const props=defineProps<{call:Pick<Call,'has_recording'|'recording_state'>}>(),state=computed(()=>recordingState(props.call)),presentation=computed(()=>recordingPresentation[state.value])
</script>
<template><span class="badge recording-badge" :class="presentation.color" :title="presentation.description"><Icon :name="presentation.icon" :size="12"/>{{recordingLabels[state]}}</span></template>
