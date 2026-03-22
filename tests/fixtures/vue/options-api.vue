<template>
  <div>
    <p>{{ message }}</p>
    <button @click="handleSubmit">Submit</button>
  </div>
</template>

<script>
export default {
  data() {
    return {
      message: '',
      items: [],
    };
  },
  methods: {
    handleSubmit() {
      if (!this.message) {
        return;
      }
      for (const item of this.items) {
        if (item.active) {
          this.message = item.label;
          break;
        }
      }
    },
    validate(value) {
      if (value === null || value === undefined) {
        return false;
      }
      if (typeof value === 'string' && value.trim() === '') {
        return false;
      }
      return true;
    },
  },
};
</script>

<style scoped>
button { cursor: pointer; }
</style>
