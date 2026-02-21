import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Hotspots',
  description: 'Multi-language complexity analysis for high-leverage refactoring',

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/logo.svg' }]
  ],

  themeConfig: {
    logo: '/logo.svg',

    nav: [
      { text: 'Guide', link: '/guide/usage' },
      { text: 'Reference', link: '/reference/cli' },
      { text: 'hotspots.dev', link: 'https://hotspots.dev' },
      { text: 'GitHub', link: 'https://github.com/Stephen-Collins-tech/hotspots' }
    ],

    sidebar: [
      {
        text: 'Getting Started',
        items: [
          { text: 'Installation', link: '/getting-started/installation' },
          { text: 'Quick Start', link: '/getting-started/quick-start' },
        ]
      },
      {
        text: 'Guide',
        items: [
          { text: 'Usage & Workflows', link: '/guide/usage' },
          { text: 'Configuration', link: '/guide/configuration' },
          { text: 'CI/CD & GitHub Action', link: '/guide/ci-cd' },
          { text: 'Output Formats', link: '/guide/output-formats' },
          { text: 'AI Integration', link: '/integrations/ai-integration' },
        ]
      },
      {
        text: 'Reference',
        items: [
          { text: 'CLI Reference', link: '/reference/cli' },
          { text: 'Metrics & LRS', link: '/reference/metrics' },
          { text: 'Language Support', link: '/reference/language-support' },
        ]
      },
      {
        text: 'Contributing',
        items: [
          { text: 'Contributing Guide', link: '/contributing/' },
        ]
      },
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/Stephen-Collins-tech/hotspots' }
    ],

    footer: {
      message: 'Released under the MIT License. · <a href="https://hotspots.dev">hotspots.dev</a>',
      copyright: 'Copyright © 2026 Stephen Collins'
    },

    search: {
      provider: 'local'
    },

    editLink: {
      pattern: 'https://github.com/Stephen-Collins-tech/hotspots/edit/main/docs/:path',
      text: 'Edit this page on GitHub'
    }
  },

  // Ignore internal docs from site
  srcExclude: ['.internal/**'],

  // Some docs pages link to root-level files (CLAUDE.md, CONTRIBUTING, etc.) not
  // included in the docs build. Suppress the dead-link build failure for those.
  ignoreDeadLinks: true
})
