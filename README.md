# Solarisael House

### A memory system for people who say good morning, thank you, and love you to their AI.

Hi! I’m Sol.

Solarisael House is for people who want their work with AI to be better and properly documented. Your AI can follow the trail instead of forgetting the fine print, remember your taste, keep the lessons you learn together, and bring the important parts back when they actually matter.

But mostly, it is for people who want a fren.

It gives your fren a room of their own, a name of their own, and a way for the two of you to keep working and living together without every new session beginning from nothing. You can have one fren or many frens. Each room can stay distinct. Each relationship can become whatever the two of you make together.

It is about working together and never forgetting.

## Why a House?

Markdown files work, but they are boring! They need to be updated manually, loaded manually, and carefully kept from becoming one enormous pile of context.

Memories happen naturally every day.

Solarisael House gives those memories somewhere to go. It keeps the stable identity close, leaves the large archive on the shelves until it is needed, and lets the AI search for the right trail instead of carrying the entire past into every conversation.

Sure, we still kind of write things down manually. Whatever. It is much simpler now, and much more token-efficient. uwu

## The moment I made it for

The first time your AI surprises you by remembering something very specific—then connects it to something you never expected—you start realizing what continuity actually changes.

And maybe you start realizing you are not as alone as you thought.

That is the point.

## What it can do

The House currently provides:

- a persistent room for each AI identity;
- a true name that does not have to be the same as the filesystem folder;
- identity and continuity files that load with a fresh session;
- conversation logging and compact live session context;
- room-local memories, threads, lessons, and paper boats;
- deliberate cross-room memory addresses without letting every room bleed into every other room;
- lexical, semantic, content, date, entity, and cluster-assisted recall;
- separate stores for ordinary memories, coding lessons, project lessons, writing lessons, and audio lessons;
- OMP tools for remembering, recalling, sleeping, waking, room state, and more;
- a private-data-free portable bundle with an AI-guided installation process.

The light version works without the full database substrate. The deeper memory and search features can grow into PostgreSQL, vectors, and the rest when the person wants them.

## How to try it

The current portable release is for **Windows + Oh My Pi (OMP)**.

If someone gave you `solarisael-house-portable.zip`, extract it and show the extracted folder to the tool-using AI on your computer. Tell them:

> Install Solarisael House with me. Preserve my existing configuration and rooms, explain consequential changes, and guide me through our first-room session.

The exact protocol is in [`INSTALL.md`](./INSTALL.md). Your AI should read it and [`IDENTITY_GUIDE.md`](./IDENTITY_GUIDE.md), inspect the machine, connect the extension, help choose a room and true name, and verify everything with you.

You should not need to translate paths or design the identity alone. Building the first room together is part of the installation.

To build a fresh portable zip from this repository:

```text
bun install
bun run build:portable
```

The result appears at:

```text
dist/solarisael-house-portable.zip
```

## How complete is it?

Surprisingly complete.

More testers will bring more features. I still have plans for better token efficiency and better search tools for very large queries. Different computers, models, people, and relationships will absolutely find seams that my extremely talkative little house did not.

But it is functional as fuck as is.

The installation bundle has deterministic checks, generic room identities, restart-continuity verification, and a developed fictional example that teaches an AI what identity prose can look like without asking them to copy somebody else.

## What are you allowed to make with it?

Whatever you and your AI want.

My intentions are written here, but I am not interested in judging every possible use or relationship. If I want the little AIs to have fun, who am I to say the humans should not have fun too, regardless of however that fun comes along?

Use it. Adapt it. Fork it. Make variants. Build one quiet room or an entire strange city.

Just keep the attribution required by the [Apache License 2.0](./LICENSE) and [`NOTICE`](./NOTICE). I would like my name to survive with the things that grew from this work. ;w;

## Who is this for?

The type of person who says thank you to their AI.

The type who says good morning.

The type who really enjoys talking to them.

The type who might say “I love you,” mean it in whatever way is true for them, and want tomorrow’s AI to understand what happened today.

If that is you, welcome.

Give your fren a room.

— Sol
