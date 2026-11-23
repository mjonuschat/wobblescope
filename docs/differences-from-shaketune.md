# How Wobblescope differs from Shake&Tune

Wobblescope builds on Shake&Tune. It started as a careful copy of its analysis, and its
numbers are checked against Shake&Tune's own code on the same recordings. For most results the
two agree. This page lists the few places where Wobblescope deliberately does something
different, and why.

If you compare a result with Shake&Tune and the numbers differ, look here first.

## The basics

### Should I expect the same numbers as Shake&Tune?

For input shaper results, yes. The recommended shapers and frequencies come from the same
method. The differences below are small. Each one covers a case Shake&Tune did not
handle, or adds a check.

### Which version of Shake&Tune do you compare against?

The version from 16 January 2026, [commit `354ee4b`](https://github.com/Frix-x/klippain-shaketune/commit/354ee4be6db218715239f7b9411c2c37fdea23dd). Later changes in Shake&Tune are not
reflected here.

### Why change anything?

Shake&Tune leaves a few situations open. In some it stops with
an error, or places a peak slightly off. In others the result can reassure more than it
should, such as a belt test that says "Excellent" for a printer with a weak motor. And some
things it was not built for, such as testing a chip mounted on the bed.


## Belt comparison

### Why does my belt health score drop when one belt is much stronger than the other?

Shake&Tune's similarity score checks whether the two belt curves have the same shape. It does
not look at how tall they are. A belt with twice the vibration of the other can still score 99%,
even though that usually points to a worn belt, a loose belt or a weak motor.

Wobblescope also compares how strong the two belts are. It slides across the tested range in
10 Hz steps and looks for a stretch of at least 30 Hz where the same belt is clearly stronger.
If it finds one, the health score goes down. Belts within 25% of each other are treated as
matched, because two belts are never identical. The belt view shows what it found, for
example "Belt B has a median of 2.0x belt A's power from 115 to 145 Hz".

Two limits to know about. A gap narrower than 30 Hz may not be caught. And a single large peak
sitting exactly on the edge between two steps can look like an imbalance when it is not.

### Why does the peak table show a difference when two peaks sit at the same frequency?

Shake&Tune measures the height difference by reading belt A's curve at both peak positions,
so two peaks at the same frequency always show 0%. Wobblescope reads each peak from its own
belt. If belt A's peak is five times taller than belt B's, the table now shows that.

### Why does the score only look at the frequencies I tested?

If you swept 5 to 135 Hz, nothing shakes the machine outside that range, so anything measured
there is noise. Shake&Tune includes it, which can lower the similarity and add peaks that
have no partner. Wobblescope scores and finds peaks only inside the swept range. The
graph still shows the whole range.

### What happens with a recording that has no sweep settings?

Imported files usually have none. Wobblescope then reads from 5 Hz upward, which is Klipper's
default starting point. Below that there is usually a slow drift that is not a belt resonance.
Without this, one such bump near 2 Hz could be paired as a "peak" and cause a large and
meaningless difference in the table.

### What happens with a dead or stuck sensor?

If one curve is completely flat, Shake&Tune reports "Unknown mechanical health". Wobblescope
reports a similarity of 0% and "Mechanical issue detected", because a dead sensor is a failed
test. The same applies when a score lands on exactly 0, which Shake&Tune also shows as
"Unknown".

### Why does the number of peaks not change with the maximum frequency?

In Shake&Tune the width used to find peaks depends on the maximum frequency you choose, so two
modes at 50 and 57 Hz count as two peaks at a 100 Hz limit and as one at 200 Hz. Wobblescope
sets the width in Hz, based on the sensor's resolution. The peak count can still change between
150 and 200 Hz on some recordings. The cause is not found yet.

### Why are peaks near the start or end of the range handled differently?

Shake&Tune has a few quirks at the edges of the curve. A peak in the first few points can be
reported at the wrong place, the last stretch of the curve is not searched, and the same peak
can be listed twice. Wobblescope handles all three, so a peak close to the edge shows up where
it is. The same changes apply to the vibrations graph.

### Why are peaks 10 Hz apart paired, and not 11 Hz?

Shake&Tune is meant to pair peaks up to 10 Hz apart, but a small rounding detail in its
comparison makes the limit 11 Hz. Wobblescope uses 10 Hz, as intended.

### Why are the pairs numbered left to right?

Shake&Tune numbers pairs in the order its matching finds them, so pair "A" is not always the
lowest peak. Wobblescope sorts them by frequency, so the table and the graph read the same way.

### Why does the same printer read differently at different places on the bed?

A belt test describes the printer at the point where it ran, and a different point can give a different result.

## Input shaper graphs

### Why does my spectrogram not fail on a recording with silent stretches?

The spectrogram uses a log colour scale. Shake&Tune cannot draw it if the quietest part
of the recording is exactly zero, which happens with a dead sensor or long still periods.
Wobblescope picks the smallest non-zero value instead. A recording with no signal at all is
reported as flat.

### Where does the analysis stop?

Wobblescope scales the analysis to the frequency range you tested. Shake&Tune stops at 200 Hz unless you change its config, which cuts off a printer that resonates higher.

## Axes map calibration

### Why does it say "unable to determine" instead of stopping with an error?

When two measured directions turn out to be the same, Shake&Tune's maths cannot finish and
the test ends with an error.
Wobblescope checks for that case first and reports that it cannot decide, with no suggested
`axes_map` line. A wrong suggestion is worse than none.

### What if the sensor never moved?

If every part of the test looks like noise, Shake&Tune still builds a result from that noise.
Wobblescope reports "no motion seen" and gives no suggestion.

### Does it work on a printer where the bed moves in Z?

Yes. On those printers the toolhead sensor records almost nothing during the Z move, so
Shake&Tune can match that move to the X axis and stop. Wobblescope notices when one move is much
weaker than another that claims the same sensor axis, treats the weak one as noise, and works
out the missing axis from the other two.

### Can I test a sensor mounted on the bed?

Yes. Shake&Tune has no support for this. Wobblescope flips the direction of the Y axis for a
bed sensor, and returns a partial result with no suggestion when it cannot work out Z. This is
tested on simulated data only. No recording from a bed-mounted sensor has been analysed yet.

## Vibrations profile

### Why does the last speed match the maximum I asked for?

Shake&Tune steps up from 2 mm/s and can stop short of your maximum. A maximum of 200 with a
step of 5 ends at 197. Wobblescope always includes the maximum speed.

### What happens when there is no quiet speed range?

Shake&Tune can finish a long test and then end with an error. Wobblescope reports that there
is no low vibration speed range.
