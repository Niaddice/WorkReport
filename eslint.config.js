import js from '@eslint/js'
import pluginVue from 'eslint-plugin-vue'
import tseslint from 'typescript-eslint'
import prettier from 'eslint-config-prettier'
import globals from 'globals'

export default [
  { ignores: ['dist/**', 'node_modules/**', 'src-tauri/**', '.release/**'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs['flat/recommended'],
  {
    files: ['**/*.{ts,vue}'],
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
      parserOptions: { parser: tseslint.parser, extraFileExtensions: ['.vue'] },
    },
    rules: {
      'no-undef': 'off',
      // 用户输入校验需要显式匹配控制字符，避免写入不可见内容。
      'no-control-regex': 'off',
      'vue/multi-word-component-names': 'off',
      // Material Web 组件使用原生 slot 属性，不是已废弃的 Vue 2 语法。
      'vue/no-deprecated-slot-attribute': 'off',
      // TypeScript 的可选属性已经表达了默认值语义，无需再强制声明。
      'vue/require-default-prop': 'off',
      '@typescript-eslint/no-explicit-any': 'off',
    },
  },
  prettier,
]
