import { defineRule } from '@oxlint/plugins';

export const typeImportsFirstRule = defineRule({
  meta: {
    type: 'layout',
    fixable: 'code',
    docs: {
      description: 'Put inline type imports before value imports in named import lists.',
    },
    messages: {
      typesFirst: 'Put type imports before value imports in the named import list.',
    },
  },
  createOnce(context) {
    return {
      ImportDeclaration(node) {
        if (node.importKind === 'type') {
          return;
        }

        const named = [];
        const types = [];
        const values = [];

        let valueSeen = false;
        let misplacedType = false;

        for (const specifier of node.specifiers) {
          if (specifier.type !== 'ImportSpecifier') {
            continue;
          }

          named.push(specifier);

          if (specifier.importKind === 'type') {
            types.push(specifier);
            misplacedType ||= valueSeen;
          } else {
            values.push(specifier);
            valueSeen = true;
          }
        }
        
        if (!misplacedType) {
          return;
        }

        context.report({
          node,
          messageId: 'typesFirst',
          fix(fixer) {
            if (context.sourceCode.getCommentsInside(node).length > 0) {
              return null;
            }
            const ordered = [...types, ...values];
            return named.map((specifier, index) => {
              const replacement = ordered[index];
              if (!replacement) {
                return null;
              }
              return fixer.replaceText(specifier, context.sourceCode.getText(replacement));
            });
          },
        });
      },
    };
  },
});
