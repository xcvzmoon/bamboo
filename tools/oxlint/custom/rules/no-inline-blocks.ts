import { type ESTree, defineRule } from '@oxlint/plugins';

type CodeBlock =
  | ESTree.BlockStatement
  | ESTree.ClassBody
  | ESTree.StaticBlock
  | ESTree.TSModuleBlock;

export const noInlineBlocksRule = defineRule({
  meta: {
    type: 'layout',
    docs: {
      description: 'Require multiline code blocks and braced conditional and loop bodies.',
    },
    messages: {
      multiline: 'Use a multiline block with its body on separate lines between the braces.',
      braces: 'Use a braced multiline block for this branch or loop body.',
    },
  },
  createOnce: (context) => {
    function checkLayout(
      node: ESTree.Node,
      openingLine: number,
      closingLine: number,
      members: readonly ESTree.Node[],
    ) {
      let previousLine = openingLine;

      for (const member of members) {
        if (member.loc.start.line <= previousLine) {
          context.report({ node, messageId: 'multiline' });
          return;
        }

        previousLine = member.loc.end.line;
      }

      if (closingLine <= previousLine) {
        context.report({ node, messageId: 'multiline' });
      }
    }

    function checkBlock(node: CodeBlock) {
      const opening = context.sourceCode.getFirstToken(node, {
        filter: (token) => token.value === '{',
      });

      if (opening !== null) {
        checkLayout(node, opening.loc.start.line, node.loc.end.line, node.body);
      }
    }

    function requireBlock(body: ESTree.Statement) {
      if (body.type !== 'BlockStatement') {
        context.report({ node: body, messageId: 'braces' });
      }
    }

    return {
      BlockStatement: checkBlock,
      ClassBody: checkBlock,
      StaticBlock: checkBlock,
      TSModuleBlock: checkBlock,
      IfStatement(node) {
        requireBlock(node.consequent);

        // Else-if chains are checked by their own IfStatement visitor.
        if (node.alternate !== null && node.alternate.type !== 'IfStatement') {
          requireBlock(node.alternate);
        }
      },
      ForStatement(node) {
        requireBlock(node.body);
      },
      ForInStatement(node) {
        requireBlock(node.body);
      },
      ForOfStatement(node) {
        requireBlock(node.body);
      },
      WhileStatement(node) {
        requireBlock(node.body);
      },
      DoWhileStatement(node) {
        requireBlock(node.body);
      },
      SwitchStatement(node) {
        // Start after the discriminant so an object or function in it cannot
        // be mistaken for the switch's opening brace.
        let opening = context.sourceCode.getTokenAfter(node.discriminant);

        while (opening !== null && opening.value !== '{') {
          opening = context.sourceCode.getTokenAfter(opening);
        }

        if (opening !== null) {
          checkLayout(node, opening.loc.start.line, node.loc.end.line, node.cases);
        }

        for (const branch of node.cases) {
          const colon = context.sourceCode.getFirstToken(branch, {
            filter: (token) =>
              token.value === ':' &&
              (branch.test === null || token.range[0] >= branch.test.range[1]),
          });
          
          let previousLine = colon?.loc.end.line;

          for (const statement of branch.consequent) {
            if (previousLine !== undefined && statement.loc.start.line <= previousLine) {
              context.report({ node: statement, messageId: 'multiline' });
              break;
            }

            previousLine = statement.loc.end.line;
          }
        }
      },
    };
  },
});
