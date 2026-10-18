<script setup lang="ts">
/** App mark: a rounded tile holding a hexagonal "box". */
</script>

<template>
  <svg
    class="app-logo"
    viewBox="0 0 24 24"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <defs>
      <linearGradient id="exmt-logo-gradient" x1="2" y1="2" x2="22" y2="22">
        <stop offset="0" stop-color="#4cc2ff" />
        <stop offset="1" stop-color="#0067c0" />
      </linearGradient>
    </defs>
    <rect x="1.5" y="1.5" width="21" height="21" rx="6" fill="url(#exmt-logo-gradient)" />
    <path
      d="M12 5.4 18.2 9v6L12 18.6 5.8 15V9z"
      stroke="#ffffff"
      stroke-width="1.5"
      stroke-linejoin="round"
      opacity="0.95"
    />
    <circle cx="12" cy="12" r="2.2" fill="#ffffff" />
  </svg>
</template>

<style scoped>
.app-logo {
  display: block;
}
</style>
