# League Stewarding

Stewarding tools for league racing. A page in your browser shows the live grid and an incident feed, with a button for every penalty, Safety Car, Virtual Safety Car and red flag controls, and a switch for the game's automatic penalties. Experimental. Needs Swofty Launcher for F1 25 1.5.13 or later.

## Using it

Turn the addon on and play as usual. When the game that hosts the race opens its session screen, the addon opens the page in your default browser once. The page lives at `http://127.0.0.1:<port>/?t=<token>`: the port is picked at random each game start, and the token is a random 128-bit value that every request must carry.

- **Grid:** position, lap, pit state, penalty seconds and warnings for every car.
- **Penalties:** warning, +5 s, +10 s, custom seconds, drive-through, stop-go and disqualification, per driver.
- **Race control:** Safety Car, Virtual Safety Car, end, and red flag, each behind a confirmation.
- **Automatic penalties:** off stops the game issuing its own penalties; manual ones still apply.
- **Event feed:** newest first, from the game's telemetry and from the actions taken on the page.

The grid, lap and event feed come from the game's UDP telemetry. Switch it on in Game Options, Settings, Telemetry Settings: UDP Telemetry On, UDP Broadcast Mode Off, IP Address 127.0.0.1, Port 20777, UDP Format 2025. The page shows this as a banner while no packets arrive.

Penalties apply while a race is running. Race control needs a session with a Safety Car.

## How it works

`native/steward.dll` binds the game's own penalty functions and runs the page's requests on the game thread, so the game's own code issues every penalty and race control change.

| Request | What runs |
|---|---|
| `GET /api/state` | Grid and events from `telemetry.cpp`, race control state from the SafetyCarLogic entry |
| `POST /api/penalty` | `Sess_Issue_Warning`, `Sess_Issue_TimePenalty`, `Sess_Issue_DriveThrough`, `Sess_Issue_StopGo`, `Sess_Issue_GridPlacePenalty` or `Sess_Issue_Disqualify`, called at the top of `Sess_CornerCutting_Update` |
| `POST /api/race-control` | A transition of the safety car state machine, run after the stock `Sess_SafetyCar_VehicleTransition` tick |
| `POST /api/auto-penalties` | While off, the six issuers return early when `Sess_CornerCutting_Update` is the caller |

Every request needs the token, in `?t=` or the `X-Steward-Token` header, and a `Host` of `127.0.0.1` or `localhost` on the page's port. The panel is embedded in the DLL.

## Layout

| Path | What |
|---|---|
| `native/` | The in-game plugin. Build with `native/build.sh`; host tests run with `native/test/run.sh`. |
| `web/` | The page: `index.html`, `app.js`, `style.css`. |
| `icons/` | The addon icon and its SVG source. |
