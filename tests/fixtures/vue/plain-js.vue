<template>
  <div>{{ label }}</div>
</template>

<script>
function computeLabel(value) {
  if (value > 100) {
    return 'high';
  } else if (value > 50) {
    return 'medium';
  } else {
    return 'low';
  }
}

function clamp(value, min, max) {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}
</script>
