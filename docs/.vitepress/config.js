import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Hotspots',
  description: 'Multi-language complexity analysis for high-leverage refactoring',

  themeConfig: {
    logo: '/logo.svg',

    nav: [
      { text: 'Guide', link: '/guide/usage' },
      { text: 'Reference', link: '/reference/metrics' },
      { text: 'GitHub', link: 'https://github.com/Stephen-Collins-tech/hotspots' }
    ],

    sidebar: {
      '/getting-started/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'Installation', link: '/getting-started/installation' },
            { text: 'Quick Start', link: '/getting-started/quick-start' },
            { text: 'React Projects', link: '/getting-started/quick-start-react' }
          ]
        }
      ],

      '/guide/': [
        {
          text: 'User Guide',
          items: [
            { text: 'CLI Usage', link: '/guide/usage' },
            { text: 'Configuration', link: '/guide/configuration' },
            { text: 'CI Integration', link: '/guide/ci-integration' },
            { text: 'GitHub Action', link: '/guide/github-action' },
            { text: 'Suppression', link: '/guide/suppression' },
            { text: 'Output Formats', link: '/guide/output-formats' }
          ]
        }
      ],

      '/reference/': [
        {
          text: 'Reference',
          items: [
            { text: 'Metrics', link: '/reference/metrics' },
            { text: 'LRS Specification', link: '/reference/lrs-spec' },
            { text: 'CLI Reference', link: '/reference/cli' },
            { text: 'JSON Schema', link: '/reference/json-schema' },
            { text: 'Language Support', link: '/reference/language-support' },
            { text: 'Limitations', link: '/reference/limitations' }
          ]
        }
      ],

      '/architecture/': [
        {
          text: 'Architecture',
          items: [
            { text: 'Overview', link: '/architecture/overview' },
            { text: 'Design Decisions', link: '/architecture/design-decisions' },
            { text: 'Invariants', link: '/architecture/invariants' },
            { text: 'Multi-Language', link: '/architecture/multi-language' },
            { text: 'Testing', link: '/architecture/testing' }
          ]
        }
      ],

      '/contributing/': [
        {
          text: 'Contributing',
          items: [
            { text: 'Getting Started', link: '/contributing/' },
            { text: 'Development', link: '/contributing/development' },
            { text: 'Adding Languages', link: '/contributing/adding-languages' },
            { text: 'Releases', link: '/contributing/releases' }
          ]
        }
      ],

      '/integrations/': [
        {
          text: 'Integrations',
          items: [
            { text: 'MCP Server', link: '/integrations/mcp-server' },
            { text: 'AI Agents', link: '/integrations/ai-agents' }
          ]
        }
      ]
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/Stephen-Collins-tech/hotspots' }
    ],

    footer: {
      message: 'Released under the MIT License.',
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
