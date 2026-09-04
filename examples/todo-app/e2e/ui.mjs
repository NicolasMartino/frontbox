// End-to-end check of the trial UI, in a real browser.
//
// # Why this exists, and what it caught
//
// `scripts/verify.sh` gates the UI by compiling and linting it, because there is no headless Dioxus
// runtime here. That is an honest gap and it is not a small one: the nested user/todo rewrite
// shipped with **two defects a compiler cannot see**, and this file is what found them.
//
//   1. The signup gate read the projection without subscribing to the revision counter, so
//      `SessionBar` re-rendered after a signup and the gate under it did not. The page acknowledged
//      the signup and refused to move past it.
//   2. `Txn`'s `Drop` in `frontbox-indexeddb` detached its completion handlers only when aborting,
//      so every *read* transaction freed closures the browser still held and then called one —
//      an uncaught `closure invoked recursively or after being dropped` per read, forever. The
//      conformance suite stayed green throughout, because the throw happens inside the browser's
//      event dispatch where no Rust future can see it. **Only a console reader finds that.**
//
// So this asserts on JS console errors as loudly as it asserts on behaviour.
//
// # Running it
//
//     docker compose up -d --build          # or `dx serve --web` and two `cargo run`s
//     npm i -D playwright && npx playwright install chromium
//     APP_URL=http://127.0.0.1:8081 \
//     TODO_URL=http://127.0.0.1:3000 USER_URL=http://127.0.0.1:3001 node examples/todo-app/e2e/ui.mjs
//
// Restart the two servers first if they hold state from an earlier run: they are in-memory, so a
// restart is the reset, and the signup gate cannot be reached while the user service still knows
// this client's user id.
//
// # The waits are matched to `SyncCadence`, not guessed
//
// `idle_ms` is 5s and `offline_ms` is 15s (`crates/frontbox-dioxus/src/sync/cadence.rs`). The
// invalidation budget used by this browser demo is also 15s, so cross-window assertions wait one
// full budget plus a drain pass. A shorter wait fails a working application, which cost a full
// debugging pass before it was written down. The one deliberate exception is the come-back-online
// step: it waits *less* than `offline_ms` on purpose, because unticking the switch fires the app wake
// and the drain should not serve out the backoff.
//
// # Focus is stubbed, and everything downstream of it is real
//
// Section 11 drives the focus gating, and **Chromium under an automation protocol will not report a
// page as unfocused**: `document.hasFocus()` answers `true` for every page in the context, in headed
// mode as well as headless, and `bringToFront` fires no `visibilitychange`. Measured before this was
// written, because the alternative is a test that silently asserts nothing.
//
// So the init script below overrides the two properties the application reads and dispatches the
// real events. That is the posture the bfcache wake was verified under too — the browser signal is
// synthesised and every line after it is the shipping path: `session::focused()` reads exactly these
// two properties, and the wake, the pause and the resume are untouched.
import { chromium } from 'playwright';

// Checked here rather than where they are used, and the two are far apart on purpose. `TODO_URL`
// is first read in step 9, after a full CRUD run and a cascade — so an unset variable used to
// surface as `fetch failed` against the URL `undefined/api/v1/todos`, minutes in, looking like the
// service had died rather than like the harness had never been told where it was. A missing
// address is a setup mistake and belongs at startup, where it costs nothing to find.
//
// `APP_URL` is deliberately not in this list: it has a default, because the UI's address is the one
// this script can reasonably guess from `docker-compose.yml`.
for (const name of ["TODO_URL", "USER_URL"]) {
  if (!process.env[name]) {
    console.error(`${name} is not set. Run this through \`just examples e2e\`, which derives ` +
      `every address from the ports, or set APP_URL, TODO_URL and USER_URL by hand.`);
    process.exit(2);
  }
}

const app = process.env.APP_URL ?? "http://127.0.0.1:8081";
const browser = await chromium.launch();
const ctx = await browser.newContext();
// Installed on the *context* so the mirror window gets it too, and before any bundle runs. The
// defaults are the browser's own answers, so nothing changes until a test calls `__setFocus`.
await ctx.addInitScript(() => {
  let focused = true;
  Object.defineProperty(document, 'hidden', { get: () => !focused, configurable: true });
  Object.defineProperty(document, 'visibilityState', {
    get: () => (focused ? 'visible' : 'hidden'),
    configurable: true,
  });
  document.hasFocus = () => focused;
  window.__setFocus = (next) => {
    focused = next;
    document.dispatchEvent(new Event('visibilitychange'));
    if (next) window.dispatchEvent(new Event('focus'));
  };
});
const page = await ctx.newPage();
const errors = [];
const watch = page => {
  page.on('pageerror', e => errors.push(e.message));
  page.on('console', m => { if (m.type() === 'error') errors.push(`console: ${m.text()}`); });
};
watch(page);

let pass = 0, fail = 0;
const check = (name, ok, detail = '') => {
  if (ok) { pass++; console.log(`  PASS  ${name}`); }
  else { fail++; console.log(`  FAIL  ${name}  ${detail}`); }
};
// Every step reports rather than aborting the run: one broken control should not hide the state of
// the other nine.
const step = async (name, fn) => {
  try { await fn(); } catch (e) { fail++; console.log(`  FAIL  ${name}: ${e.message.split('\n')[0]}`); }
};

// Snapshot the visible model: users (name, count, open) and their todo rows.
const snapshot = async (target = page) => {
  const users = [];
  for (const li of await target.locator('li.user').all()) {
    const name = await li.locator('.user-head input.title').inputValue();
    const count = (await li.locator('.count').innerText()).trim();
    const open = (await li.locator('.expander').innerText()).trim() === '▼';
    const todos = [];
    for (const row of await li.locator('.nested li.row').all()) {
      todos.push({
        title: await row.locator('input.title').inputValue(),
        done: await row.locator('input[type=checkbox]').isChecked(),
        saving: (await row.getAttribute('class')).includes('saving'),
      });
    }
    users.push({ name, count, open, todos });
  }
  return users;
};
const status = async () => (await page.locator('.status').innerText()).replace(/\n+/g, ' · ').trim();

// Find a user row by the *value* of its name input. Dioxus sets `value` as a DOM property, so the
// `[value="..."]` attribute selector does not match it — this reads the property instead.
const rowFor = async (name, target = page) => {
  const rows = target.locator('li.user');
  for (let i = 0; i < await rows.count(); i++) {
    if (await rows.nth(i).locator('.user-head input.title').inputValue() === name) return rows.nth(i);
  }
  throw new Error(`no user row named ${name}`);
};

await page.goto(app, { waitUntil: 'networkidle' });
await page.waitForTimeout(1200);

console.log('\n== 1. the gate ==');
check('no user composer before signup', await page.getByPlaceholder("new user's name").count() === 0);
check('no todo composer before signup', await page.getByPlaceholder('what needs doing').count() === 0);
check('offline switch is present', await page.locator('.offline').count() === 1);
check('status bar is present', await page.locator('.status').count() === 1);

console.log('\n== 2. offline signup ==');
await step('go offline', () => page.locator('.offline input').check());
await page.getByPlaceholder('your name').fill('Alice');
await page.getByRole('button', { name: 'sign up' }).click();
await page.waitForTimeout(900);
// The outcome of a click is a toast now, not a line wedged into the session bar. Checked here
// rather than later because a success toast is on a four-second timer.
// The whole stack, not one toast: the startup hydrate has usually put its own success up by now,
// so a bare `.toast.success` is two elements and a strict-mode violation.
check('signup reports itself in a toast',
  (await page.locator('.toasts').innerText()).includes('queued'),
  await page.locator('.toasts').innerText().catch(() => '(no toasts)'));
let users = await snapshot();
check('gate lifted', users.length === 1, JSON.stringify(users));
check('own row named Alice', users[0]?.name === 'Alice');
check('own row tagged (you)', await page.locator('.tag').innerText() === '(you)');
check('own row starts expanded', users[0]?.open === true);
// The status bar's counts refresh on the drain loop's cadence rather than on enqueue, so this
// waits for a tick rather than reading immediately.
await page.waitForTimeout(6000);
check('queued not sent', (await status()).includes('pending 1'), await status());
check('the success toast retired itself', await page.locator('.toast.success').count() === 0,
  await page.locator('.toasts').innerText().catch(() => '(no toasts)'));

console.log('\n== 3. offline todos, then drain ==');
await page.getByPlaceholder('what needs doing').first().fill('buy milk');
await page.getByRole('button', { name: 'Add', exact: true }).first().click();
await page.waitForTimeout(500);
await page.getByPlaceholder('what needs doing').first().fill('call mum');
await page.getByRole('button', { name: 'Add', exact: true }).first().click();
await page.waitForTimeout(700);
users = await snapshot();
check('two todos projected offline', users[0]?.todos.length === 2, JSON.stringify(users[0]?.todos));
check('rows marked saving', users[0]?.todos.every(t => t.saving) === true);
check('count reads 2 todos', users[0]?.count === '2 todos', users[0]?.count);
await step('go online', () => page.locator('.offline input').uncheck());
// Deliberately shorter than `offline_ms` (15s): unticking fires the app wake, so the drain should
// happen now rather than when the backoff would have expired.
await page.waitForTimeout(3000);
users = await snapshot();
check('drained: no row still saving', users[0]?.todos.every(t => !t.saving) === true, await status());
check('queue empty', (await status()).includes('pending 0'), await status());

console.log('\n== 4. add a second user ==');
const mirror = await ctx.newPage();
watch(mirror);
await mirror.goto(app, { waitUntil: 'networkidle' });
await mirror.waitForTimeout(1800);
await page.getByPlaceholder("new user's name").fill('Bob');
await page.getByRole('button', { name: 'Add user' }).click();
await page.waitForTimeout(6000);
users = await snapshot();
check('two users', users.length === 2, JSON.stringify(users.map(u => u.name)));
check('Bob has no todos', users.find(u => u.name === 'Bob')?.count === 'no todos');
check('Bob starts collapsed', users.find(u => u.name === 'Bob')?.open === false);
await mirror.waitForTimeout(19000);
let mirrored = await snapshot(mirror);
check('second window sees Bob through polling', mirrored.some(u => u.name === 'Bob'), JSON.stringify(mirrored.map(u => u.name)));

console.log('\n== 5. expand Bob, add a todo in his name ==');
const bobRow = await rowFor('Bob');
await step('expand Bob', () => bobRow.locator('.expander').click());
await page.waitForTimeout(400);
await bobRow.getByPlaceholder('what needs doing').fill("bob's errand");
await bobRow.getByRole('button', { name: 'Add', exact: true }).click();
await page.waitForTimeout(6000);
users = await snapshot();
const bob = users.find(u => u.name === 'Bob');
check('Bob expanded', bob?.open === true);
check("Bob has his own todo", bob?.todos.length === 1 && bob.todos[0].title === "bob's errand", JSON.stringify(bob?.todos));
check("Alice's list unchanged", users.find(u => u.name === 'Alice')?.todos.length === 2);
await mirror.waitForTimeout(19000);
mirrored = await snapshot(mirror);
check('second window sees Bob todo count through polling', mirrored.find(u => u.name === 'Bob')?.count === '1 todo', JSON.stringify(mirrored));

console.log('\n== 6. todo CRUD ==');
const aliceRow = await rowFor('Alice');
await aliceRow.locator('.nested li.row').first().locator('input[type=checkbox]').check();
await page.waitForTimeout(1200);
users = await snapshot();
check('todo ticks', users.find(u => u.name === 'Alice')?.todos.some(t => t.done) === true);
await aliceRow.locator('.nested li.row').first().locator('input.title').fill('buy oat milk');
await aliceRow.locator('.nested li.row').first().locator('input.title').press('Tab');
await page.waitForTimeout(1200);
users = await snapshot();
check('todo renames', users.find(u => u.name === 'Alice')?.todos.some(t => t.title === 'buy oat milk') === true,
  JSON.stringify(users.find(u => u.name === 'Alice')?.todos));
// By accessible name rather than by the glyph. Every control in a row is now labelled with the
// todo it belongs to, and selecting on that turns this line into the only assertion this harness
// can make about accessibility: if the label regresses, the step fails rather than the coverage
// silently disappearing.
const firstTodo = aliceRow.locator('.nested li.row').first();
check('row controls carry accessible names',
  await firstTodo.getByRole('checkbox', { name: 'Done: buy oat milk' }).count() === 1
  && await firstTodo.getByRole('textbox', { name: 'Rename buy oat milk' }).count() === 1
  && await firstTodo.getByRole('button', { name: 'Delete buy oat milk' }).count() === 1,
  await firstTodo.innerHTML());
await firstTodo.getByRole('button', { name: 'Delete buy oat milk' }).click();
await page.waitForTimeout(1200);
users = await snapshot();
check('todo deletes', users.find(u => u.name === 'Alice')?.todos.length === 1);

console.log('\n== 7. rename a user ==');
await bobRow.locator('.user-head input.title').fill('Robert');
await bobRow.locator('.user-head input.title').press('Tab');
await page.waitForTimeout(6000);
users = await snapshot();
check('user renames', users.some(u => u.name === 'Robert'), JSON.stringify(users.map(u => u.name)));

console.log('\n== 8. delete a user with todos: the confirm ==');
const robert = await rowFor('Robert');
await step('arm delete', () => robert.locator('.user-head > button').last().click());
await page.waitForTimeout(300);
check('confirm appears', await page.locator('.confirm').count() === 1);
check('confirm names the cost', (await page.locator('.confirm').innerText()).includes('1 todos')
  || (await page.locator('.confirm').innerText()).includes('1 todo'), await page.locator('.confirm').innerText());
await page.getByRole('button', { name: 'no' }).click();
await page.waitForTimeout(300);
check('cancel dismisses', await page.locator('.confirm').count() === 0);
await step('arm delete', () => robert.locator('.user-head > button').last().click());
await page.waitForTimeout(200);
await page.getByRole('button', { name: 'yes' }).click();
// The delete drains on the idle cadence (5s), and the cascade is two more round trips.
await page.waitForTimeout(9000);
users = await snapshot();
check('user gone', !users.some(u => u.name === 'Robert'), JSON.stringify(users.map(u => u.name)));
check('their todo gone with them', users.reduce((n, u) => n + u.todos.length, 0) === 1, JSON.stringify(users));

console.log('\n== 9. the cascade reached the todo service ==');
const served = await (await fetch(`${process.env.TODO_URL}/api/v1/todos`)).json();
check('todo service has one todo left', served.length === 1, JSON.stringify(served));
const servedUsers = await (await fetch(`${process.env.USER_URL}/api/v1/users`)).json();
check('user service has one user left', servedUsers.length === 1, JSON.stringify(servedUsers));

console.log('\n== 10. reload keeps everything ==');
await page.reload({ waitUntil: 'networkidle' });
await page.waitForTimeout(2000);
users = await snapshot();
check('survives a reload', users.length === 1 && users[0].todos.length === 1, JSON.stringify(users));

console.log('\n== 11. the cache is legible, and the poll stops when nobody is looking ==');
// The two loops share one `Wake`. Before it was multi-waiter, one event released exactly one of
// them — measured at 5 of 6 fires — and the loser served out its full interval. The resume below is
// the assertion that covers it: it can only be prompt if this loop got the wake.
const ticks = [];
mirror.on('console', m => {
  if (m.text().includes('ui invalidation tick')) ticks.push(Date.now());
});
check('cache line names both services',
  (await mirror.locator('.status .cache').innerText()).includes('todo-service')
  && (await mirror.locator('.status .cache').innerText()).includes('user-service'),
  await mirror.locator('.status .cache').innerText());

await mirror.evaluate(() => window.__setFocus(false));
// The loop notices at the top of its next iteration, so this is up to one `TICK_MS` and not
// instant — and that is the honest ordering: until it parks, polling is not actually paused, and a
// label that said so would be ahead of the fact.
await mirror.locator('.status .paused').waitFor({ timeout: 8000 }).catch(() => {});
check('paused is said out loud', await mirror.locator('.status .paused').count() === 1,
  await mirror.locator('.status').innerText());
// Counted from after it parked, so a tick already in flight when focus was lost is not read as
// polling-while-unfocused.
const before = ticks.length;
// Three full ticks' worth. A paused loop runs no timer at all, so this is not a slower cadence.
await mirror.waitForTimeout(15000);
check('no polling while unfocused', ticks.length === before, `${ticks.length - before} ticks`);

await mirror.evaluate(() => window.__setFocus(true));
await mirror.waitForTimeout(1200);
check('polling resumes on focus', ticks.length > before, `${ticks.length - before} ticks`);
check('paused notice is gone', await mirror.locator('.status .paused').count() === 0);

// **Asserted, not merely printed.** This block used to `console.log` the errors and stop there,
// while the header above claimed the run asserted on them "as loudly as on behaviour" — so the
// exact defect this file was written to catch (the `Txn::drop` handler leak, one uncaught throw per
// IndexedDB read) would have scrolled past and exited 0. A claim a harness does not enforce is
// worse than no claim, because it is read as coverage.
const uniq = [...new Set(errors)];
console.log('\n=== JS ERRORS ===');
console.log(uniq.length ? `${uniq.join('\n')}  (${errors.length} occurrences)` : '(none)');
check('no JS console errors or page exceptions', uniq.length === 0,
  `${errors.length} occurrence(s): ${uniq.join(' | ')}`);

console.log(`\n=== ${pass} passed, ${fail} failed ===`);
await page.screenshot({ path: process.env.SCREENSHOT ?? 'e2e-final.png', fullPage: true });
await browser.close();
process.exit(fail ? 1 : 0);
