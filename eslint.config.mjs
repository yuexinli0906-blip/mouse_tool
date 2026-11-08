import antfu from "@antfu/eslint-config";
import reactHooks from "eslint-plugin-react-hooks";

export default antfu(
  {
    stylistic: false,
    react: true,
    ignores: [
      "src-tauri/**",
      "dist/**",
      "src/routeTree.gen.ts",
      // Specta writes this file. Do not lint generated bindings.
      "src/bindings.ts",
      // Third-party Morph Bot runtime; keep it byte-for-byte with upstream.
      "src/vendor/**",
    ],
  },
  {
    files: ["src/components/ui/**/*.tsx"],
    rules: {
      "react-refresh/only-export-components": "off",
    },
  },
  // React Compiler rules (eslint-plugin-react-hooks recommended-latest)
  {
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      ...reactHooks.configs["recommended-latest"].rules,
    },
  },
);
