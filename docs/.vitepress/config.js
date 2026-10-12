import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Hotspots',
  description: 'Find where your engineering attention has the highest expected value.',
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/logo.svg' }],

    // Open Graph
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:site_name', content: 'Hotspots' }],
    ['meta', { property: 'og:image', content: 'https://docs.hotspots.dev/logo.svg' }],
    ['meta', { property: 'og:image:alt', content: 'Hotspots — Multi-language complexity analysis' }],

    // Twitter / X
    ['meta', { name: 'twitter:card', content: 'summary' }],
    ['meta', { name: 'twitter:image', content: 'https://docs.hotspots.dev/logo.svg' }],
  ],

  themeConfig: {
    logo: '/logo.svg',
    // hotspots.dev's own wordmark is lowercase (Header.astro: "hotspots") — match it
    // exactly rather than defaulting to the capitalized `title` above.
    siteTitle: 'hotspots',

    nav: [
      { text: 'Quick Start', link: '/quickstart' },
      { text: 'Usage', link: '/USAGE' },
      { text: 'Reference', link: '/REFERENCE' },
      { text: 'Architecture', link: '/ARCHITECTURE' },
      { text: 'Contributing', link: '/CONTRIBUTING' },
      { text: 'hotspots.dev', link: 'https://hotspots.dev' },
      { text: 'GitHub', link: 'https://github.com/Stephen-Collins-tech/hotspots' }
    ],

    sidebar: [
      {
        text: 'Docs',
        items: [
          { text: 'Quick Start', link: '/quickstart' },
          { text: 'Usage & Workflows', link: '/USAGE' },
          { text: 'CLI & Config Reference', link: '/REFERENCE' },
          { text: 'Architecture', link: '/ARCHITECTURE' },
          { text: 'Score Stability & Predictability', link: '/RISK_STABILITY' },
          { text: 'Contributing', link: '/CONTRIBUTING' },
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
    },
  },

  // Ignore internal/planning docs from the published site (not linked from
  // nav/sidebar, but would otherwise still build as orphan pages at a guessable URL).
  srcExclude: ['.internal/**', 'requirements/**'],

  // Some docs pages link to root-level files (CLAUDE.md, CONTRIBUTING, etc.) not
  // included in the docs build. Narrowed to that known pattern (rather than a
  // blanket `true`) so genuine in-body anchor/link drift — e.g. a duplicated
  // heading or a renamed section — still fails the build instead of being silenced.
  ignoreDeadLinks: [/\/(CLAUDE|CONTRIBUTING|SECURITY|TASKS|STATUS)\.md$/]
})
