<script setup lang="ts">
defineProps<{ name: string }>();
</script>

<template>
  <svg
    class="icon" :class="`icon-${name}`"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <template v-if="name === 'home'">
      <path d="M3 11.5 12 4l9 7.5" />
      <path d="M5.5 9.5V20h13V9.5" />
      <path d="M10 20v-5.5h4V20" />
    </template>
    <template v-else-if="name === 'book'">
      <path d="M12 6.5C10 5 7 4.5 3.5 4.5v14c3.5 0 6.5.5 8.5 2 2-1.5 5-2 8.5-2v-14C17 4.5 14 5 12 6.5Z" />
      <path d="M12 6.5v14" />
    </template>
    <template v-else-if="name === 'download'">
      <path d="M12 3.5v11" />
      <path d="m7 10 5 5 5-5" />
      <path d="M4.5 19.5h15" />
    </template>
    <template v-else-if="name === 'upload'">
      <path d="M12 15.5v-11" />
      <path d="m7 9 5-5 5 5" />
      <path d="M4.5 19.5h15" />
    </template>
    <template v-else-if="name === 'map'">
      <path d="M3.5 6.5 9 4.5l6 2 5.5-2v13l-5.5 2-6-2-5.5 2Z" />
      <path d="M9 4.5v13" />
      <path d="M15 6.5v13" />
    </template>
    <template v-else-if="name === 'box'">
      <path d="M12 3 20 7.5v9L12 21l-8-4.5v-9Z" />
      <path d="m4 7.5 8 4.5 8-4.5" />
      <path d="M12 12v9" />
    </template>
    <template v-else-if="name === 'sliders'">
      <path d="M4 7h9" />
      <path d="M19 7h1" />
      <circle cx="16" cy="7" r="2.5" />
      <path d="M4 17h1" />
      <path d="M11 17h9" />
      <circle cx="8" cy="17" r="2.5" />
    </template>
    <template v-else-if="name === 'layers'">
      <path d="M12 3.5 21 8l-9 4.5L3 8Z" />
      <path d="m3 12 9 4.5 9-4.5" />
      <path d="m3 16 9 4.5 9-4.5" />
    </template>
    <template v-else-if="name === 'offline'">
      <path d="M3 3l18 18" />
      <path d="M8.5 16a5 5 0 0 1 6.2-.8" />
      <path d="M5 12.5a10 10 0 0 1 4-2.3" />
      <path d="M14.5 10.2A10 10 0 0 1 19 12.5" />
      <path d="M2 9a14.5 14.5 0 0 1 4.3-2.6" />
      <path d="M11 5.1A14.5 14.5 0 0 1 22 9" />
      <path d="M12 19.5h.01" />
    </template>
    <template v-else-if="name === 'play'">
      <rect x="3" y="4" width="18" height="16" rx="2.5" />
      <path d="M3 8.5h18" />
      <path d="M10.5 11.5v5.5l4.5-2.75Z" />
    </template>
    <template v-else-if="name === 'code'">
      <path d="m8.5 7-5 5 5 5" />
      <path d="m15.5 7 5 5-5 5" />
    </template>
    <template v-else-if="name === 'file'">
      <path d="M6 3h8.5L19 7.5V21H6Z" />
      <path d="M14 3v5h5" />
      <path d="M9 13h6" />
      <path d="M9 17h4" />
    </template>
    <template v-else-if="name === 'chip'">
      <rect x="6.5" y="6.5" width="11" height="11" rx="1.5" />
      <path d="M10 10h4v4h-4Z" />
      <path d="M9.5 3v3.5M14.5 3v3.5M9.5 17.5V21M14.5 17.5V21M3 9.5h3.5M3 14.5h3.5M17.5 9.5H21M17.5 14.5H21" />
    </template>
    <template v-else-if="name === 'tag'">
      <path d="M3.5 12.5v-8h8l9 9-8 8Z" />
      <circle cx="8" cy="8.5" r="1.5" />
    </template>
    <template v-else-if="name === 'alert'">
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5v5.5" />
      <path d="M12 16.5h.01" />
    </template>
    <template v-else-if="name === 'flag'">
      <path d="M5.5 21V4" />
      <path d="M5.5 4.5h13l-2.5 4.25 2.5 4.25h-13" />
    </template>
    <template v-else-if="name === 'bookmark'">
      <path d="M18 21l-6-4.5L6 21V4.5A1.5 1.5 0 0 1 7.5 3h9A1.5 1.5 0 0 1 18 4.5Z" />
    </template>
    <template v-else-if="name === 'chevron'">
      <path d="m9 6 6 6-6 6" />
    </template>
    <template v-else-if="name === 'menu'">
      <path d="M3.5 6.5h17M3.5 12h17M3.5 17.5h17" />
    </template>
    <template v-else-if="name === 'panel-left-close'">
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <path d="M9 3v18" />
      <path d="m16 15-3-3 3-3" />
    </template>
    <template v-else-if="name === 'panel-left-open'">
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <path d="M9 3v18" />
      <path d="m14 9 3 3-3 3" />
    </template>
    <template v-else-if="name === 'moon'">
      <path d="M20.5 13.5A8.5 8.5 0 1 1 10.5 3.5a6.5 6.5 0 0 0 10 10Z" />
    </template>
    <template v-else-if="name === 'feather'">
      <path d="M20 12.5a6 6 0 0 0-8.5-8.5L5 10.5V19h8.5Z" />
      <path d="M16 8 2.5 21.5" />
      <path d="M17.5 15H9" />
    </template>
    <template v-else-if="name === 'umbrella'">
      <path d="M22 12a10 10 0 0 0-20 0Z" />
      <path d="M12 12v6.5a2.5 2.5 0 0 0 5 0" />
    </template>
    <template v-else-if="name === 'search'">
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m15.5 15.5 5 5" />
    </template>
    <template v-else-if="name === 'x'">
      <path d="M6 6l12 12M18 6 6 18" />
    </template>
    <template v-else-if="name === 'struct'">
      <path d="M12 3 20 7.5v9L12 21l-8-4.5v-9Z" />
      <path d="m4 7.5 8 4.5 8-4.5" />
      <path d="M12 12v9" />
    </template>
    <template v-else-if="name === 'func'">
      <path d="m8.5 7-5 5 5 5" />
      <path d="m15.5 7 5 5-5 5" />
    </template>
    <template v-else-if="name === 'data'">
      <path d="M5 9h14M5 15h14M10 4 8 20M16 4l-2 16" />
    </template>
    <template v-else-if="name === 'site' || name === 'call' || name === 'vtable'">
      <circle cx="12" cy="12" r="7.5" />
      <path d="M12 2.5v4M12 17.5v4M2.5 12h4M17.5 12h4" />
    </template>
    <template v-else-if="name === 'arrow'">
      <path d="M5 12h14" />
      <path d="m13 6 6 6-6 6" />
    </template>
    <template v-else-if="name === 'scale'">
      <path d="M12 4v16" />
      <path d="M8 20h8" />
      <path d="M5 7h14" />
      <path d="m5 7-2.5 6a3 3 0 0 0 5 0Z" />
      <path d="m19 7-2.5 6a3 3 0 0 0 5 0Z" />
    </template>
    <template v-else-if="name === 'windows'">
      <path d="M3.5 5.5 10.5 4.5v7h-7Z" />
      <path d="M13.5 4 20.5 3v8.5h-7Z" />
      <path d="M3.5 12.5h7v7l-7-1Z" />
      <path d="M13.5 12.5h7V21l-7-1Z" />
    </template>
    <template v-else-if="name === 'linux'">
      <path d="M4 5.5h16v13H4Z" />
      <path d="m7.5 10 2.5 2-2.5 2" />
      <path d="M12 14.5h4" />
    </template>
    <template v-else-if="name === 'chef'">
      <path d="M6.5 14a4 4 0 0 1 .9-7.9 5 5 0 0 1 9.2 0 4 4 0 0 1 .9 7.9" />
      <path d="M6.5 14v6h11v-6" />
      <path d="M6.5 17h11" />
    </template>
    <template v-else-if="name === 'deck'">
      <rect x="2.5" y="7" width="19" height="10" rx="5" />
      <rect x="8" y="9" width="8" height="6" rx="1" />
      <circle cx="5.5" cy="12" r="1" />
      <circle cx="18.5" cy="12" r="1" />
    </template>
  </svg>
</template>
