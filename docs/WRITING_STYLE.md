# Documentation style

All documents in this repository use ASD-STE100 (Simplified Technical
English). Write for a reader who has no previous knowledge of ShallGuard,
requirements engineering, or coding agents. This page gives the rules.

## Sentences

- Write short sentences. Use a maximum of 20 words in an instruction and a
  maximum of 25 words in a description.
- Put one topic in one sentence. Put one instruction in one sentence.
- Use the active voice. Say who or what does the action. Write "the check
  reports an error", not "an error is reported".
- Use the present tense for descriptions. Use the imperative for
  instructions: "Run the check", not "You should run the check" or "The check
  should be run".
- Use "must" for a rule. Use "can" for a possibility or a permission. Do not
  use "should", "may", "might", or "could" in prose.
- Keep the words SHALL, SHALL NOT, and MAY only inside a requirement
  statement, where RFC 2119 defines them. Do not use them in ordinary text.
- Do not use dashes, semicolons, or parentheses to join two thoughts. Start a
  new sentence.
- Do not start a sentence with a verb form that ends in "-ing". Do not use
  such forms as verbs ("checking the link" becomes "the tool checks the
  link").
- Use the articles "a", "an", and "the". Do not remove them to make a sentence
  shorter.
- Do not put more than three nouns in a row. Write "the root of the
  repository", not "repository root directory path".

## Paragraphs and lists

- Put one topic in one paragraph. Use a maximum of six sentences in a
  paragraph.
- Start a paragraph with the most important sentence.
- Use a vertical list for steps and for sets of parallel items. Use a
  numbered list for steps that have an order.
- Put a warning or a caution in its own sentence, before the step it applies
  to.

## Words

- Use one word for one thing. For example, always write "requirement", never
  "contract", "SHALL statement", "clause", or "spec item" for the same thing.
  The [glossary](GLOSSARY.md) lists the approved terms.
- Use simple words: "use" (not "utilize"), "start" (not "initiate"), "make
  sure" (not "ensure"), "for example" (not "e.g."), "that is" (not "i.e.").
- Do not use idioms, metaphors, jokes, or slang. Write "the tool finds the
  missing tests", not "the tool digs up the missing tests".
- Do not use words with two meanings in the same document. For example, do
  not use "check" both as a noun for the command and as a verb for what a
  person does. Write "run the check" and "examine the result".
- Explain each technical term the first time you use it, or link to the
  glossary. This includes terms from Rust and Git, such as "crate", "Cargo
  workspace", "merge request", and "CI".
- Write numbers as digits. Write "3 crates", not "three crates".
- Spell out an abbreviation the first time you use it: "continuous integration
  (CI)".

## Structure of a document

- Start with one paragraph that says what the document is for and who reads
  it.
- Use headings that name the topic. Do not use questions or jokes as
  headings.
- Keep code, commands, and file paths in code formatting. Do not put a
  command inside a sentence.
- Keep a code block or a table out of the flow of a sentence. Introduce it
  with a full sentence that ends with a colon.

## Why this style, and how to help

English is not the native language of the maintainer. The
[Code of Conduct](../CODE_OF_CONDUCT.md#language-and-editorial-contributions)
explains this, and it welcomes help with grammar, clarity, tone, and wording.
Simplified Technical English gives every contributor the same clear rules, so
that nobody has to rely on a native feel for the language.

An editorial pull request is a valued contribution. If a text reads unusual,
blunt, or ambiguous, assume good intent and ask. An editorial change to a
requirement statement must keep its technical meaning. Treat a change of
meaning as a specification change, not as a grammar correction.
