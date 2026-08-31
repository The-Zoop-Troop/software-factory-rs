// Every user-facing explainer in one place: page intros, section help, metric
// definitions, empty states, and the glossary. Components import from here so
// the console speaks with one voice and the copy can be reviewed in bulk.

export interface GlossaryEntry { readonly term: string; readonly def: string }

export const PAGE = {
  overview: {
    desc: 'Each card is one rig — a repository with its own planner and autonomous workers. Open a rig to submit plans, watch epics land, and answer anything that needs you.',
  },
  rig: {
    desc: 'Everything this rig is doing: items that need a human, plans being broken down, epics in flight, and a feed of events as they happen.',
  },
  epic: {
    desc: 'One epic end to end — its tasks and their budgets, the plan that created it, where it came from, and everything that has happened so far.',
  },
  throughput: {
    desc: 'How this epic actually ran: overall totals, every attempt laid out by stage, and where the time went.',
  },
} as const;

export const SECTION_HELP = {
  needsYou: 'Work the factory cannot finish without a human decision. Pick an option or write a note — the next worker session reads it before it starts.',
  planning: 'Plan requests queued for this rig’s planner. Each one is broken down into an epic of small, independently verified tasks.',
  epics: 'An epic is one plan broken into tasks. The bar fills as tasks pass verification and land on the main branch.',
  completed: 'Epics that reached a terminal state. Open one to revisit its full history and metrics.',
  liveFeed: 'Events streamed from the rig in real time — claims, verifications, landings, incidents.',
  alerts: 'Webhook and chat notifications the console delivered while this session has been connected.',
  tasks: 'Every task the planner created for this epic. Click a row for full detail: branch, budgets, verify commands, and the worker’s notes.',
  timeline: 'This session’s events for the epic, newest first.',
  plan: 'What was asked (plan text), what workers were told (references), and the contract describing what actually landed.',
  provenance: 'Where this epic came from, and which other plans build on what it landed.',
  gantt: 'One lane per task. Colors are stages, pale segments are waits, and faded segments are attempts that did not land.',
  stages: 'Time spent in each stage across every attempt: sample count, median, worst case, and total.',
  totals: 'Wall-clock is elapsed time; work is the sum of active stage time; parallelism is work ÷ wall-clock. The critical path is the longest dependent chain of tasks — more workers cannot beat it. Retry tax is time spent on attempts that did not land.',
  rollup: 'Wall-clock is elapsed time; work is summed active time; parallelism is work ÷ wall-clock. First pass counts tasks that landed on their first attempt; retry tax is time spent on attempts that did not land.',
  budget: 'What this task may spend versus what it has used. A worker that exhausts a budget stops and asks for help instead of burning on.',
  attention: 'Every item that needs a human, across all rigs. The count in the header is the one glanceable “does anything need me?” signal.',
  accessToken: 'An operator token scopes what you can see and do (watch, plan, resolve). Paste it here — it is kept only in this browser.',
} as const;

export const EMPTY = {
  needsYou: 'Nothing needs your attention. When a worker exhausts a budget, hits a conflict, or needs a decision, it appears here.',
  attention: 'Nothing needs you right now. Items from every rig appear here the moment one does.',
  epics: 'Nothing in flight. Submit a plan above and the planner will break it into an epic of verified tasks.',
  rigsOffline: 'Connect with an operator token to see your rigs.',
  rigsNone: 'No rigs are visible to this credential. Ask whoever runs the factory for one with rig scopes.',
} as const;

export const GLOSSARY: ReadonlyArray<GlossaryEntry> = [
  { term: 'Rig', def: 'One repository plus the planner and workers that operate on it. Rigs are independent; a plan in one rig can wait on an epic in another.' },
  { term: 'Plan', def: 'A short request in plain language — what you want built. The rig’s planner turns it into an epic.' },
  { term: 'Epic', def: 'A plan broken into small tasks, each with its own budgets and verify commands. An epic is done when every task has landed.' },
  { term: 'Task', def: 'One unit of work with a branch, budgets, and verify commands. Workers claim tasks, attempt them, and land the result on the main branch.' },
  { term: 'Attempt', def: 'One worker session on a task: work, then verification, then integration. A failed attempt is retried until the attempt budget runs out.' },
  { term: 'Verify', def: 'The commands that must pass before a task’s work can land. Their output is kept in the task’s notes.' },
  { term: 'Landed', def: 'The task’s work passed verification and was integrated into the main branch.' },
  { term: 'Needs you', def: 'The factory hit something it will not decide alone — a spent budget, a merge conflict, an ambiguous instruction. It waits for your option or note.' },
  { term: 'Contract', def: 'A record of what an epic actually landed. Downstream plans that waited on it read the contract before they start.' },
  { term: 'Lease', def: 'A worker’s time-limited claim on a task. When a lease expires the task is free for another worker.' },
  { term: 'Budget', def: 'Hard limits per task — tokens, attempts, wall-clock. Exhausting one stops the work and asks a human instead.' },
  { term: 'Posture', def: 'Whether the rig is currently running, paused, or stopped.' },
  { term: 'Throughput', def: 'The epic’s efficiency picture: wall-clock versus work, parallelism, critical path, and retry tax.' },
] as const;
