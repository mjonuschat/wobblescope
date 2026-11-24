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

In Shake&Tune the maximum frequency you choose changes how close two peaks can be before they
merge. Peaks at 50 and 57 Hz show as two with a 100 Hz limit and as one with a 200 Hz limit.
Wobblescope finds the same peaks whatever maximum you choose. It also ignores small bumps that are
only noise on a flat stretch of a curve.

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

## Input shaper graphs

### Why does my spectrogram not fail on a recording with silent stretches?

The spectrogram uses a log colour scale. Shake&Tune cannot draw it if the quietest part
of the recording is exactly zero, which happens with a dead sensor or long still periods.
Wobblescope picks the smallest non-zero value instead. A recording with no signal at all is
reported as flat.

### Where does the analysis stop?

Wobblescope scales the analysis to the frequency range you tested. Shake&Tune stops at 200 Hz unless you change its config, which cuts off a printer that resonates higher.

### Why does the shaper search go above 150 Hz?

Wobblescope searches for shapers up to the end of the frequency range you tested, if that is above 150 Hz. Shake&Tune stops at 150 Hz, so on a stiff printer the best shaper lands on 149.8 Hz and the real resonance is missed.

### Why does my graph show more peaks than Shake&Tune's?

Wobblescope smooths the spectrum less than Shake&Tune does, so it finds more weak peaks, mostly above 100 Hz. The thresholds are the same as Shake&Tune's: 5% of the strongest value for a marker and 12% for one that matters.

### Why does the result warn that the best shaper is at the edge of the range?

Wobblescope warns when the recommended shaper is the highest frequency it tried, because the real resonance may be higher.

### Why is my damping ratio higher than in Shake&Tune?

The damping ratio is estimated from the width of the main resonance peak. Shake&Tune measures the width where the spectrum has fallen to the peak divided by the square root of two. Wobblescope measures it at half the peak instead.

Wobblescope also stops measuring the width where the spectrum turns up again, by at least a tenth of the main peak, towards a second peak. Shake&Tune measures both peaks as one wide one.

The higher ratio changes the shaper table and the `damping_ratio` lines in the config snippet.

### Why is a shaper in the table with a warning and no numbers?

A shaper's formulas are only valid up to a damping ratio: 0.2 for 3hump_ei and 0.3 for 2hump_ei. A heavily damped resonance gives an estimated ratio above that. Wobblescope then leaves that shaper out of the search and lists it with a warning, instead of stopping the whole analysis. A ratio you enter yourself is not changed, so a shaper that cannot take it is still an error.

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

### Can I measure more directions than the two motor directions?

Shake&Tune measures the two motor directions and computes every other angle from them.
Wobblescope has an "Angular resolution" setting: motor directions only (the default, which gives
the same result as Shake&Tune), or every 45, 22.5 or 15 degrees. Measured directions are drawn
from their recorded steps and labelled measured; the others are labelled computed, estimated
from the measured ones. A run takes longer in proportion to the number of directions, and
the form shows the estimate before you start.
