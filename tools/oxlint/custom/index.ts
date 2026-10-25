import { eslintCompatPlugin } from "@oxlint/plugins";

import { noInlineBlocksRule } from "./rules/no-inline-blocks.ts";
import { requireMultilineObjectPropertiesRule } from "./rules/require-multiline-object-properties.ts";
import { typeImportsFirstRule } from "./rules/type-imports-first.ts";

const customPlugin = eslintCompatPlugin({
  meta: {
    name: "custom",
  },
  rules: {
    "no-inline-blocks": noInlineBlocksRule,
    "require-multiline-object-properties": requireMultilineObjectPropertiesRule,
    "type-imports-first": typeImportsFirstRule,
  },
});

export default customPlugin;