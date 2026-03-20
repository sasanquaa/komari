# This content is AI-generated with provision

- [Download](#download)
- [Concepts](#concepts)
  - [Map](#map)
  - [Movement](#movement)
  - [Characters](#characters)
  - [Action](#action)
      - [Normal and Priority](#normal-and-priority)
      - [Configuration](#configuration)
  - [Linked Key & Linked Action](#linked-key--linked-action)
  - [Rotation Modes](#rotation-modes)
    - [Auto-mobbing](#auto-mobbing)
    - [Ping Pong](#ping-pong)
  - [Platforms Pathing](#platforms-pathing)
  - [Run Timer](#run-timer)
  - [Capture Modes](#capture-modes)
  - [Familiars Swapping](#familiars-swapping)
  - [Panic Mode](#panic-mode)
  - [Elite Boss Spawns Behavior](#elite-boss-spawns-behavior)
  - [Notifications](#notifications)
  - [Localization](#localization)
  - [Generic/HEXA Booster](#generichexa-booster)
  - [HEXA Booster Exchange](#hexa-booster-exchange)
- [Video Guides](#video-guides)
- [Showcase](#showcase)
  - [Rotation](#rotation)
  - [Auto Mobbing & Platforms Pathing](#auto-mobbing--platforms-pathing)
  - [Rune Solving](#rune-solving)

## Download

1. Go to the [GitHub Release Page](https://github.com/sasanquaa/komari/releases)
2. Download `app-release.zip` or `app-debug.zip`
3. Extract the archive
4. Run the `.exe` file

## Concepts

### Map

- The map is detected automatically but must be created manually by providing a name.
- The created map is saved and can be selected later.
- Any action presets created in the detected map are saved to that map only.

Setup steps:
1. Navigate to the map where you wish to train.  
2. Wait for the bot to automatically detect the map.  
3. If any issues occur, refer to the [troubleshooting guide](https://github.com/sasanquaa/komari/blob/master/docs/troubleshooting.md).  
4. Click the `Create` button and assign a name (for example, use the same name as the map).  
5. Repeat the above steps for each new map you wish to train in.  

> **Note:**  
> The arcs shown are for visualization purposes only. They do **not** represent the actual movement path. However, they indicate the sequence of actions, as shown by their numbering.

![Map](https://github.com/sasanquaa/komari/blob/master/.github/images/map.png?raw=true)

---

### Movement

Default bot movement (without platform pathing) follows these steps:

1. Moves horizontally to match the destination’s `x` coordinate.
2. Then performs a fall, up-jump, or grapple to match the destination’s `y` coordinate.

If the bot is close enough to the destination (within `25` units, subject to change), it will **walk** instead of performing a double jump.

---

### Characters

- The `Characters` tab is used to change key bindings, set up buffs, and more.
- Character profiles can be created separately for each character.
- Characters are saved globally and are not tied to a specific map.

#### Character Sections

1. `Key Bindings` – For general in-game key mappings.
2. `Use potion and feed pet` – Configures potion usage and pet feeding.
3. `Use booster` – Configures Generic/HEXA booster usage.
4. `Movement` – Movement settings.
5. `Familiars` – Familiars swapping settings.
6. `Buffs` – For automatic buff setup.
7. `Fixed Actions` – Shared across all maps (useful for buffs or one-time skills).
8. `Others` – Miscellaneous character settings.

##### Potion Mode

There are two modes available for configuring potion usage:

- `EveryMillis` – Uses a potion every `x` milliseconds.  
- `Percentage` – Uses a potion when HP drops below a certain percentage.

##### Movement

- `Up jump is flight` — Treats up jump as flight (e.g., Illium).
- `Jump then up jump if possible` — Jumps first, then up jumps if available (e.g., Night Lord).
- `Has extended teleport range` — Teleportation has extended range (e.g., Explorer Mages).
- `Disable grapple on double jumping` - Disables performing a grapple during double jumping.
- `Disable teleport on fall` – Disables teleport after falling (useful for mage classes).
- `Disable double jumping` – Disables the `DoubleJumping` state (e.g., makes the bot only walk).  
  - Works only if the action does not have `Use with = DoubleJump`.
- `Disable grapple on double jumping` — Prevents grappling during double jump.
- `Disable walking` – Disables the `Adjusting` state (forces horizontal movement by double jumps only).  
  - Works only if the action does not have `Adjust` ticked.

From **v0.12**, the `Rope Lift` skill can now be disabled. If not provided, the bot will attempt to up-jump instead.

> **Note:**  
> For supported buffs, the bot relies on detecting buffs in the top-right corner of the screen.

![Buffs](https://github.com/sasanquaa/komari/blob/master/.github/images/buffs.png?raw=true)

From **v0.21**, if multiple conflicting buffs (e.g., x2/x3 EXP coupons, small/large WAP) are enabled,  
the bot will prevent buffing one if it detects the other is active. Currently, it does **not** prioritize which buff to use first.

---

### Action

Can be configured under `Actions` tab. Action is the bot main mechanism for using skills, solving rune, buffing, etc.

There are two types of actions:

- `Move` – Moves to a specific location on the map.  
- `Key` – Uses a key (optionally with a position).

Setup steps:  
1. Can be accessed after creating the map you want to train in (e.g. see [Map](#map)).  
2. Add `Normal`, `Erda Shower off cooldown`, or `Every milliseconds` actions to that map.  
3. Configure each action as either `Move` or `Key` based on your needs.  
4. Click `Start` below the minimap UI to begin.  
5. Repeat for any additional maps you plan to train in.


#### Normal and Priority

Actions are categorized as either **normal** or **priority**:

- **Priority action** overrides normal action temporarily.  
- Normal action resumes once the priority action is complete.

Current priority actions include:
- `Erda Shower off cooldown`
- `Every milliseconds`

> **For `Erda Shower off cooldown` to work:**  
> - The Erda Shower skill must be assigned to a quick slot.  
> - Action customization must be toggled on and visible.  
> - The skill must actually cast when triggered, or the bot will rerun the actions chain.

![Erda Shower](https://github.com/sasanquaa/komari/blob/master/.github/images/erda.png?raw=true)

#### Configuration

For `Move` action:
- `Adjust` – Ensures the actual position matches the target closely.  
  - When enabled, it overrides the `Disable walking` option and allows walking.  
- `X` – Horizontal coordinate to move to.  
- `X range` – Adds randomization: `[x - range, x + range]`.  
- `Y` – Vertical coordinate to move to.  
- `Wait after move` – Delay (in milliseconds) after moving (e.g., for looting).  
- `Linked action` – See [Linked Key & Linked Action](#linked-key--linked-action).  
  - Can only be used if it is not the first action or the list is non-empty.

For `Key` action:
- `Positioned` – Determines if the key action is position-dependent.  
- `X / X range / Y / Adjust / Linked action` – Same as Move Action.  
- `Key` – The key to press.  
- `Use count` – Number of times to use the key.  
- `Hold for` – Duration (ms) to hold the key.  
- `Holding buffered` – Buffers the hold duration on the last key use count.  
  - Requires `Wait after buffered` enabled and no link key.  
  - Allows holding the key while moving (e.g., Ren class).  
  - Adds the hold duration to `Wait after use` when active.
- `Has link key` – Enables link key (useful for combo classes).  
- `Queue to front` – For priority actions only.  
  - Allows this action to override non-queue-to-front priority actions.  
  - The overridden action is delayed, not lost.  
  - Useful for `press attack after X ms even while moving`.  
  - Cannot override linked actions.  
- `Use direction` – Sets the direction for the action.  
- `Use with` - Uses the key with specific player's state.  
  - `Any` – Performs as appropriate.  
  - `Stationary` – Only when standing (for buffs).  
  - `DoubleJump` – With double jump.  
- `Wait before / Wait after` – Delay (ms) before/after using the key (applies to each repeat).  
- `Wait random range` – Adds randomness to the wait time: `[delay - range, delay + range]`.
- `Wait after buffered` – Buffers the post-use wait time on the last key use count to let the next action begin earlier.  
  - `None` – No buffering; waits fully in place (default).  
  - `Interruptible` – Next `Key` action can interrupt the buffered wait.  
  - `Uninterruptible` – Next `Key` action waits until the buffered wait finishes.  
  - In `Uninterruptible` mode, only user-defined actions are blocked - built-in bot actions (e.g., rune solving) may still interrupt.

Actions can be reordered using the up/down icons.

![Actions](https://github.com/sasanquaa/komari/blob/master/.github/images/actions.png?raw=true)

---

### Linked Key & Linked Action

Useful for combo-oriented classes such as Blaster, Cadena, Ark, Mercedes, etc. Animation cancel timings
depend on the class, which can be configured in `Characters` → `Others` → `Link key timing`.

#### Link Key Types

- `Before` – Uses the link key before the main key (e.g., Cadena’s Chain Arts: Thrash).  
- `AtTheSame` – Uses both keys simultaneously (e.g., Blaster skating).  
- `After` – Uses the link key after the main key (e.g., Blaster Weaving/Bobbing).  
- `Along` – Uses the link key along with the main key while the link key is being held down (e.g., in-game Combo Key).

> **Notes:**  
> - Even for `AtTheSame`, the link key is sent **first**.  
> - Linked key can also be simulated via linked actions.
> - For Blaster, if Bobbing/Weaving cancellation is required, a linked action that sends `Jump` key should be added.  

#### Linked Actions

You can chain actions by enabling `Linked action` on subsequent ones.  
The first action starts the chain:

![Linked Actions](https://github.com/sasanquaa/komari/blob/master/.github/images/linked_actions.png?raw=true)

Linked actions appear visually connected with vertical bars. Once a chain begins, it cannot be overridden by any other actions.

---

### Rotation Modes

Rotation mode defines how **normal actions** are executed (priority actions are unaffected).  
You can select the mode in `Actions` → `Rotation`.

#### Available Modes

- `StartToEnd` – Runs actions from start to end and repeats.  
- `StartToEndThenReverse` – Runs from start to end, then reverses order.  
- `AutoMobbing` – Ignores normal actions; automatically detects and attacks mobs.  
- `PingPong` – Ignores normal actions; moves and attacks between bounds.

Priority actions (`Every milliseconds` and `Erda Shower off cooldown`) still follow their own logic.

#### Auto-Mobbing

Auto-mobbing targets random mobs detected on screen. To use it, sets the rotation mode to `AutoMobbing` and updates
the mobbing bound.

For platform-based movement, see [Platforms Pathing](#platforms-pathing). If platforms are added:
- Uses them to better estimate mob positions.
- Identifies platform "gaps" to avoid invalid mob locations.
- Adding platforms is encouraged but not required. Without them, the bot relies more on randomness.

From **v0.18.0**, Auto-mobbing now follows a fixed **clockwise** path, improving mob count on large maps.

How it works:
1. The bound is divided into four quadrants.  
2. The player moves between quadrants clockwise.  
3. Only mobs in the current quadrant are targeted.  
4. If no mobs remain, it moves to the next quadrant.

![Auto-mobbing](https://github.com/sasanquaa/komari/blob/master/.github/images/auto_mobbing.png?raw=true)

From **v0.21.0**, two new options are added:
- `Auto mobbing uses key when pathing` – Uses mobbing key while moving between quadrants and mobs are detected ahead.  
- `Detect mobs when pathing every` – Sets mobs detection interval when moving between quadrants.

#### Ping Pong

Introduced in **v0.12**.

Ping pong makes the player double jumps and attacks inside the bound, reversing direction upon reaching the bound edge. To use it, sets the rotation mode to `PingPong` and updates the mobbing bound.

This mode will try to force the player to always be within the bound. If player is already inside the bounds:
- May grapple or up-jump when below bound's `y` midpoint.  
- May fall when above bound's `y` midpoint.  
- Within `9` units of bound's `y` midpoint, there will be no random movement.

Simpler than Auto-mobbing; suitable for classes that primarily jump and spam attacks (e.g., Night Walker).

---

### Platforms Pathing

Supported for Auto-mobbing and Rune solving. This feature helps pathing across platforms, with
or without the `Rope Lift` skill. To use, add platforms for the selected map starting from ground level.
Use hotkeys to add them quickly.

> **Note**:
> Adding platforms improves Auto-mobbing movement.

---

### Run Timer

Introduced in **v0.24**. (previously known as `Run/stop cycle`)

Found under `Settings` → `Run timer`. When enabled, the bot runs for the specified duration, stop
and return to town. **As such, the key binding for `To town` must be set.**

The `Suspend` button allows pausing temporarily without resetting timer.

---

### Capture Modes

Found in `Settings` → `Capture` → `Mode`.

Defines how the bot captures the game image. Three modes available:

1. `BitBlt`.  
2. `Windows 10 (1903 and up)` - Default mode.  
3. `BitBltArea` – Captures a fixed region.  
   - Useful for VMs, capture cards, or Sunshine/Moonlight setups.  
   - The capture area must always include the game window.  
   - The capture area can stay behind the game but cannot be minimized.  
   - **The game must always be contained inside the capture area even when the game resizes.**  
   - **Key inputs are sent to the focused window above the capture area**.

You can also directly select a capture window via `Handle`.

---

### Familiars Swapping

Introduced in **v0.13**.

Located under `Characters` → `Familiars`. Automatically checks equipped familiar levels and swaps them when maxed:

- `Swap check every` – Interval in milliseconds between checks.  
- `Swappable slots`.  
  - `All` – All slots.  
  - `Last` – Only last slot.  
  - `SecondAndLast` – Second and last slots only.  
- `Can swap rare familiars` – Allows rare ones to be swapped.  
- `Can swap epic familiars` – Allows epic ones to be swapped.

> **Notes:**  
> - After swapping, familiar buff will be turned off. To enable familiar buff again, enables it in the `Buffs` tab.  
> - All familiar slots must be unlocked, and the familiar menu key binding must be set.
> - After 3 swapping attempts with no remaining swappable familiars, the bot stops all further swapping.

---

### Panic Mode

Introduced in **v0.14**.

Located under `Settings` → `Panic mode`.

If another player (friend, guildmate, or stranger) appears on the same map for 15 seconds,  
the bot enters `Panicking` state and cycles through channels until a map without any other player is found. 
Requires `Change channel` key binding.

From **v0.18**, if `Stop actions on fail or map change` is also enabled, the bot stops and goes to town upon failure. 
Requires `To town` key binding.

---

### Elite Boss Spawns Behavior

Introduced in **v0.17**, previously known as `Change channel on Elite Boss`.

Found under `Characters` → `Elite Boss spawns behavior`.

Available behaviors:
- `None` – No action.  
- `CycleChannel` – Changes channel when an Elite Boss appears.  
- `UseKey` – Triggers a key (useful for origin skills).

---

### Notifications

Provide a **webhook URL** for notifications in `Settings` → `Notifications`.

Available notification types:
- Rune Spawns  
- Elite Boss Spawns  
- Player Dies  
- Guildie Appears  
- Stranger Appears  
- Friend Appears  
- Detection Fails / Map Changes

If `Discord ping user ID` is set, the bot pings that user in the notification message.

---

### Localization

Introduced in **v0.22**.

This feature helps replace certain detection resources that may differ across game regions.

How to use:
1. Opens the `Localization` tab and refer to the table to find the function corresponding to each template.  
2. Makes sure the matching UI element is currently visible in your game.  
3. Identifies whether the template to replace is **color** or **grayscale**.  
4. Clicks `Capture color` or `Capture grayscale`, depending on the template type.  
5. Opens the `datasets` folder (located in the same directory as the `.exe` file).  
6. Crops the captured image to match the template, then click `Replace` button.  

---

### Generic/HEXA Booster

Introduced in **v0.22**

The Generic and HEXA Boosters can be accessed under `Characters → Use booster`.

Each booster type (Generic or HEXA) can be enabled independently. Any booster item that is not HEXA is referred
to as Generic (e.g. VIP Booster, Gilded Clockwork, etc.).

How the bot uses:
1. Attempts to use the booster.  
2. If successful, repeats from step 1.  
3. If the bot fails 5 consecutive attempts or no booster is available, it stops attempting to use boosters.

#### HEXA Booster Exchange

To enable automatic HEXA Booster exchange, the following setup is required:

- HEXA Booster is assigned to a visible quick slot.  
- The Sol Erda tracker menu is open and visible on screen.  
- The HEXA Matrix is assigned to a quick menu and visible.

There are three condition types for automatic exchange:

- `None` — Disabled; no exchange occurs.  
- `Full` — Exchanges only when Sol Erda is full.  
- `AtLeastOne` — Exchanges when at least one Sol Erda is available.

You can configure the amount to exchange using the `Amount` input field or by enabling the `Exchange all` checkbox.

> **Note:**  
> Automatic exchange will only occur when:
> - No HEXA Booster is currently available, **and**  
> - Sol Erda matches the selected exchange condition.

![HEXA Booster Exchange](https://github.com/sasanquaa/komari/blob/master/.github/images/hexa_booster_exchange.png?raw=true)


## Video Guides

From v0.16 — the first two videos are outdated but still useful for basics.

1. [Basic Operations](https://youtu.be/8X2CKS7bnHY?si=3yPmVPaMsFEyDD8c)  
2. [Auto-Mobbing and Platforms Pathing](https://youtu.be/8r2duEz6278?si=HTHb8WXh6L7ulCoE)  
3. Rotation Modes, Linked Key & Linked Actions (TODO)  
   - [Clockwise Rotation Example](https://youtu.be/-glx3b0jGEY?si=nuEDmIQTuiz3LtIq)

## Showcase (Examples from v0.1)

### Rotation
https://github.com/user-attachments/assets/3c66dcb9-7196-4245-a7ea-4253f214bba6

(This Blaster rotation was before Link Key & Link Action were added)

https://github.com/user-attachments/assets/463b9844-0950-4371-9644-14fad5e1fab9

### Auto-Mobbing & Platforms Pathing
https://github.com/user-attachments/assets/3f087f83-f956-4ee1-84b0-1a31286413ef

### Rune Solving
https://github.com/user-attachments/assets/e9ebfc60-42bc-49ef-a367-3c20a1cd00e0
