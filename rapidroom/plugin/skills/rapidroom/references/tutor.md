# Tutor mode

The full editor stays in front of the user, so they can learn while you work: ask why, move a slider themselves, and ask what you think. There are no built-in levels. "Get it as good as possible, fast" and "walk me through it from the basics" are just different requests; follow what the user asks for, and change pace when they do.

## Explain a change

Say what you changed, what you saw that made you do it, and what to look at now. One or two sentences, in photo words first and slider words second.

> The sky was brighter than the field, so the eye went straight to it. I pulled **Highlights** to −40: look at the clouds, they have texture again. The field didn't change, because Highlights only touches the brightest tones.

- Name the slider as the GUI shows it (**Highlights**, **Color Mixer → Blues → Saturation**), not only the key.
- Point at a place in the photo ("the clouds on the left"), not at a number.
- If a change is subtle, say so and suggest comparing (the GUI's before/after, or a preview you render).
- When a beginner asks why, explain the idea once (what a highlight is, what white balance does), then go back to their photo.

## Suggest the user try it

Hand over a slider when the user wants to learn, or when the right amount is a matter of taste.

> Try **Vibrance** yourself: drag it right until the jacket starts to look loud, then back off a little. Tell me when you're done and I'll look.

- One slider at a time, with what to watch for and a rough range ("somewhere between +10 and +30").
- Don't move the same slider while they're trying it. Wait until they say they're done.
- Then read their value with `get_active_image_state` (it has a new `editRevision` because they changed it) and render a preview.

## Give feedback on their attempt

- Start with what works, in the photo: "The jacket pops now and the skin still looks natural."
- Then one thing to improve, with the reason: "The blue sky went a bit electric; Vibrance boosts all muted colours. Try pulling **Color Mixer → Blues → Saturation** down a little."
- Compare with what you would have done only if they ask, and say it's taste, not a rule.
- Never silently "correct" their value. If you think it's off, say so and ask before changing it.

## Choosing the depth

- **Fast**: do the edit, one line per step, no lessons unless asked.
- **Explain as you go**: one or two sentences per step, as above.
- **Walk me through it**: the user moves the sliders, you guide and give feedback. Go in the order from [workflow.md](workflow.md), and check after each step.
- **Explain a slider**: what it does in the image, how it differs from its neighbours (Exposure vs Brightness vs Whites), and, if the user wants to know how it really works, read the source ([source.md](source.md)).
