<template>
  <div>
    <ul>
      <li v-for="item in items" :key="item.id">{{ item.name }}</li>
    </ul>
  </div>
</template>

<script setup lang="ts">
interface Item {
  id: number;
  name: string;
  active: boolean;
  score: number;
}

async function fetchItems(url: string): Promise<Item[]> {
  try {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`HTTP error: ${response.status}`);
    }
    const data = await response.json();
    return data;
  } catch (err) {
    console.error('fetch failed', err);
    return [];
  }
}

function filterAndRank(items: Item[], threshold: number): Item[] {
  const result: Item[] = [];
  for (const item of items) {
    if (!item.active) {
      continue;
    }
    if (item.score >= threshold) {
      result.push(item);
    } else if (item.score >= threshold / 2) {
      if (item.name.startsWith('priority')) {
        result.push(item);
      }
    }
  }
  result.sort((a, b) => b.score - a.score);
  return result;
}

function formatLabel(item: Item): string {
  switch (item.score) {
    case 100:
      return `${item.name} (perfect)`;
    case 0:
      return `${item.name} (zero)`;
    default:
      return item.score > 50 ? `${item.name} (high)` : `${item.name} (low)`;
  }
}
</script>
