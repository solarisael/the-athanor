# vault

Vault retrieval without a database. The configured files stay the authority. One request builds its whole index in memory.

### BM25F scoring

- `rank` scores each document over 7 weighted fields: path 4.2, title 3.8, heading 3.4, tags 2.8, keys 2.6, metadata 1.4, body 1.0.
- Each field carries its own length normalization, from 0.2 for the path to 0.75 for the body. The room may retune both tables.
- The term frequency saturates at 2.2 over 1.2. The document frequency comes from the index of this request.
- An exact phrase adds the field weight again: 2.25 outside the body, and 1.5 inside it.
- The whole query counts as a phrase. Each quoted part counts as a phrase. A phrase needs 3 characters.
- A compound query term must match, or the document drops out. A compound term holds 4 characters and one separator.
- Results sort by score, then by source path, then by heading path. Eight results come back, unless the room names another count.
- Each result carries the score, the term coverage, the matched and missing terms, the reasons, and an excerpt.
- The reasons name the fields that matched, and the fields that held an exact phrase.
- An excerpt holds 900 characters, unless the room names another size. A long body clips around the first matched term, with an ellipsis on each cut side.

### documents and chunks

- One file becomes many documents. A document is the unit that receives a score.
- Markdown splits at headings 1 to 6. The heading path joins the ancestors with " > ".
- Text before the first heading takes the heading `__preamble__`.
- Frontmatter becomes its own document, and its key names go into the keys field.
- A JSON file splits at the top level. Each leaf flattens into one `pointer: value` line.
- The object key names of a JSON record go into the keys field. The pointer goes into the metadata field.
- A JSONL file splits per line, and each heading names the line number.
- A malformed JSON file gives a warning and no documents. Malformed JSONL lines give a warning with their count.
- Any other eligible file becomes `__document__` chunks.
- A body over 6000 characters splits, with 400 characters of overlap. The room may name other sizes. Later chunks number their heading.

### the file walk

- Only five extensions enter: md, markdown, json, jsonl, and txt.
- Fifteen directory names never open, among them `.git`, `node_modules`, `target`, `dist`, and `build`.
- The walk refuses every symbolic link, and it records the refusal as a warning.
- The walk refuses a file that resolves outside its configured root.
- Secret names never open: `.env` files, key and certificate suffixes, lock files, and any name part `secret` or `credential`.
- A file over the byte limit is skipped, with a warning. An unreadable file also warns.
- The walk stops at the file limit, and it says that the results cover only the scanned part.
- The file list sorts by path, so the same Vault returns the same result.
- A root that is missing, or that is not a plain directory, gives a warning and no files.

### the tokenizer

- Text normalizes with NFKD. Combining marks drop. Everything becomes lowercase.
- A token holds letters, digits, and these marks: `_ : + # . / -`. Leading and trailing marks drop.
- A compound token also yields its parts, so `coding#446` matches `coding` and `446`.
- A query drops one-character terms and stopwords. The list holds English and Portuguese words.
- A query of stopwords only keeps all of its terms.

### glob and gitignore matching

- Each root reads its own `.gitignore`. The configured rules come after those lines.
- `**` crosses directories. `*` and `?` stop at a slash. Matching ignores case.
- A leading slash anchors the rule to the root. Any other rule matches at any depth.
- A trailing slash limits the rule to directories. A leading `!` returns the path.
- The last rule that matches decides. A comment line and an empty line do nothing.

### config

- The room directory holds `.solarisael-room.json`. A missing or broken marker gives the defaults.
- `vaultRoots` names the roots. The default is the room directory. A repeated root drops.
- A relative root joins the room directory. `.` and `..` resolve without touching the disk.
- `vaultIgnore` adds ignore rules.
- `vaultMaxFileBytes` defaults to 512 KiB, and it accepts 16 KiB to 4 MiB.
- `vaultMaxFiles` defaults to 5000, and it accepts 1 to 50000.
- `vaultMaxResults` defaults to 8, and it accepts 1 to 100.
- `vaultExcerptChars` defaults to 900, and it accepts 80 to 20000.
- `vaultChunkChars` defaults to 6000, and it accepts 500 to 200000.
- `vaultChunkOverlap` defaults to 400, and it accepts 0 to half of the chunk size, so the chunk walk always advances.
- `vaultFieldTuning` retunes named fields: `path`, `title`, `heading`, `keys`, `tags`, `body`, `metadata`.
- A tuned field takes `weight`, from 0 to 100, and `lengthNormalization`, from 0 to 1. An unnamed field keeps the table in the code.
- A value outside its range returns the default.

### the recall door

- `recall` is the only public function of the crate.
- It takes the room directory, the room name, and the query.
- An empty query, a relative directory, or a name that differs from the directory name gives an error with a code.
- The room directory must be a plain directory, and never a symbolic link.
- One call builds the config, the index, and the ranking. Nothing survives the call.
- The result names `vault-files` as its source and its authority, and it lists the roots, the file count, and the document count.
- The canon, semantic, content, and date fields stay empty. The taxonomy is static.
- The crate opens no database, and it writes nothing into a Vault.
