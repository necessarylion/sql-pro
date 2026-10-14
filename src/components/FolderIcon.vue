<script setup lang="ts">
import type { Component } from 'vue'
import {
  BoxIcon,
  BriefcaseIcon,
  CloudIcon,
  DatabaseIcon,
  FlaskConicalIcon,
  FolderIcon,
  GlobeIcon,
  HouseIcon,
  RocketIcon,
  ServerIcon,
  ShieldIcon,
  StarIcon,
} from 'lucide-vue-next'
import type { FolderColor, FolderIconName } from '../constants/folders'

// The icon of a folder, in the colour of the folder.
defineProps<{ icon: FolderIconName; color: FolderColor; size: number }>()

const ICONS: Record<FolderIconName, Component> = {
  folder: FolderIcon,
  server: ServerIcon,
  database: DatabaseIcon,
  cloud: CloudIcon,
  briefcase: BriefcaseIcon,
  home: HouseIcon,
  rocket: RocketIcon,
  flask: FlaskConicalIcon,
  shield: ShieldIcon,
  star: StarIcon,
  globe: GlobeIcon,
  box: BoxIcon,
}
</script>

<template>
  <!-- The hex values of the colours are the `--folder-*` tokens in style.css. -->
  <component :is="ICONS[icon]" :size="size" :style="{ color: `var(--folder-${color})` }" />
</template>
