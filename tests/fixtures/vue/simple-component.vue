<template>
  <div>
    <p>{{ message }}</p>
  </div>
</template>

<script setup lang="ts">
function greet(name: string): string {
  if (name) {
    return `Hello, ${name}!`;
  }
  return 'Hello!';
}

function add(a: number, b: number): number {
  return a + b;
}
</script>

<style scoped>
div { color: red; }
</style>
