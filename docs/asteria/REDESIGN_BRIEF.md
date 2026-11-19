# Project: Omacraft → Asteria

You are working on my Minecraft launcher, currently named **Omacraft**, which is being rebranded and redesigned as **Asteria**.

Repository: https://github.com/star-ges/omacraft

The launcher is already essentially feature-complete. Your job is not to reinvent its functionality, but to plan a comprehensive visual and UX redesign that makes it feel like a natural, cohesive part of my personal Linux desktop environment.

**Your immediate task is to investigate, evaluate and propose a plan. Do not implement any changes yet.**

## Project separation and repository safety

Asteria is a new, independent project derived from Omacraft.

The original Omacraft repository must remain intact and publicly available as a historical project.

Original repository:
https://github.com/star-ges/omacraft

Proposed new repository:
https://github.com/star-ges/asteria

### Requirements

- Do not rename, delete, archive, overwrite or push changes to the original Omacraft repository.
- Treat Omacraft as the source codebase, not the destination for Asteria development.
- Asteria must have its own working directory and independent GitHub repository.
- Prefer preserving the existing Git history when establishing the new repository.
- Ensure Asteria's Git remote does not point to Omacraft before any push.
- Do not create repositories, change Git remotes, push commits or perform other repository operations without my explicit approval.
- Include repository separation, application renaming and migration considerations in the implementation plan.
- Identify any existing Omarchy-specific dependencies, branding, configuration paths or assumptions that should be removed.
- Preserve existing user data and application functionality wherever possible.
- Do not assume that all occurrences of the old name can be safely replaced without examining their purpose.

### Planning-stage restriction

During this initial planning exercise, inspect the existing codebase but do not create the new repository or perform the migration.

Include a proposed repository migration procedure in `ASTERIA_REDESIGN_PLAN.md`, then wait for approval.

## 1. Understand the existing application

First, thoroughly inspect the repository.

- Identify the technology stack, architecture, UI framework, styling system and application structure.
- Map the existing features, workflows, screens and interactions.
- Understand how the frontend communicates with the launcher's underlying functionality.
- Identify reusable components and any existing design abstractions.
- Find all Omacraft/Omarchy-specific branding, references, assets and assumptions.
- Identify technical constraints that could affect a redesign.

Treat the application's existing functionality as established and valuable. Preserve it unless there is a compelling reason to change something, and explain any such proposal.

## 2. Study my desktop design language

I have an existing design brief describing the visual philosophy and conventions of my desktop environment.

**Locate and read this design brief before making design recommendations. If you cannot locate it, ask me for its path.**

Do not rely exclusively on the written brief. Inspect the relevant desktop configuration and implementation files to understand how the design philosophy has actually been realised.

Where available, investigate:

- Colour palette, typography, spacing and visual hierarchy.
- Window decorations, borders, corner radii and shadows.
- Quickshell components, menus, panels, launchers and dialogs.
- Interaction patterns, hover states, animations and transitions.
- Iconography and visual assets.
- The relationship between application surfaces and the desktop compositor.
- Existing design tokens, shared styles or reusable components.

Distinguish deliberate design conventions from incidental implementation details.

The objective is not to imitate the desktop superficially. It is to understand its underlying design principles and translate them into a coherent application.

## 3. Reimagine the launcher as Asteria

Imagine that Asteria was originally conceived and designed specifically for this desktop environment.

Evaluate the existing interface from that perspective.

For every major screen or workflow, consider:

- Does its current layout make sense within the desktop's design language?
- Is the information hierarchy appropriate?
- Could navigation be simplified or made more intuitive?
- Are there opportunities to reduce visual clutter?
- Would different component structures better express the design philosophy?
- Are existing interactions consistent with the rest of the desktop?
- Could the application feel more native through better integration with Linux desktop conventions?

**Do not assume the existing layout must be preserved.**

You have permission to propose substantial changes to screen composition, navigation, component structure and interaction design.

Equally, do not redesign things merely for novelty. If an existing layout is already effective, retain it and explain why.

The goal is a cohesive, polished application, not change for its own sake.

## 4. Establish Asteria's visual identity

The new name is **Asteria**.

It is inspired by my online identity, Star, and its association with stars and astronomy.

I want the name to inform the application's identity without turning it into an overtly space-themed or gimmicky interface.

Explore how Asteria can have a recognisable identity while remaining faithful to my desktop's design language.

Consider:

- Application icon and branding direction.
- Typography and wordmark treatment.
- Appropriate use of astronomical motifs, if any.
- Colour and accent usage.
- Splash screens, loading states and empty states.
- Consistent terminology and interface copy.

Prioritise restraint, clarity and consistency. Do not introduce decorative elements that conflict with the desktop's philosophy.

Identify all areas requiring renaming, including user-visible branding, application metadata, packaging, documentation and internal identifiers.

Distinguish cosmetic renaming from changes that could affect compatibility, user data, configuration paths or existing installations.

## 5. Preserve functionality and reliability

This is a redesign of a working launcher, not a greenfield project.

Maintain feature parity across the application.

Pay particular attention to:

- Minecraft installation and launching.
- Instance and version management.
- Authentication and account handling.
- Modding and any supported integrations.
- Settings, configuration and persistence.
- Downloads, progress reporting and error handling.
- Any additional functionality discovered during inspection.

This list is illustrative, not an assumption about which features are implemented.

Avoid unnecessary changes to backend logic or working infrastructure.

If architectural changes would materially improve the UI or maintainability, propose them separately and explain the benefits, risks and migration implications.

## 6. Produce a design and implementation plan

Before making changes, prepare a detailed proposal for my review.

Structure it as follows:

### A. Current-state assessment
Summarise the application's architecture, features, UI structure, strengths and weaknesses.

### B. Desktop design language
Summarise the principles extracted from my design brief and desktop implementation. Reference the relevant files.

### C. Proposed Asteria design direction
Describe the visual identity, design system, navigation philosophy and overall experience.

### D. Screen-by-screen redesign
For each major screen:

- Explain what currently exists.
- Identify what works and what doesn't.
- Describe your proposed design.
- Explain why it better fits the desktop.
- Classify the change as cosmetic, structural or behavioural.

Where appropriate, compare a conservative restyling approach against a more ambitious redesign.

### E. Component and architecture changes
Identify reusable components, design tokens, styling changes and any necessary restructuring.

### F. Rebranding and migration
Provide a complete inventory of naming changes, assets, application identifiers and compatibility considerations.

### G. Implementation roadmap
Break the work into logical, reviewable phases.

For each phase, specify:

- Objectives.
- Files or components likely to be affected.
- Dependencies.
- Risks.
- Testing and validation requirements.
- Expected visible outcome.

Prioritise changes that establish the design foundation before detailed screen work.

### H. Decisions requiring my input
Identify genuine design trade-offs or ambiguities where my preferences would materially affect the result.

Offer reasoned recommendations rather than simply presenting options without guidance.

## 7. Working principles

- **Plan first. Do not modify code until I approve the plan.**
- Read the actual implementation rather than making assumptions.
- Treat my desktop design brief as the primary design authority.
- Use the existing desktop implementation to validate your interpretation.
- Prioritise coherence over visual novelty.
- Preserve working functionality.
- Avoid unnecessary dependencies and architectural complexity.
- Prefer reusable design primitives over one-off styling.
- Consider keyboard navigation, accessibility, responsiveness and different window sizes.
- Respect the existing project's conventions where they do not conflict with the redesign.
- Be willing to challenge existing UI decisions, but justify proposed alternatives.
- Distinguish essential changes from optional improvements.
- Do not introduce Omarchy branding, dependencies or design assumptions into Asteria.

## 8. Deliverables

Create a Markdown planning document in the repository, named `ASTERIA_REDESIGN_PLAN.md`.

The document should be detailed enough to guide a subsequent implementation, but readable enough for me to evaluate the proposed direction without needing to inspect every source file.

Include references to relevant source files and desktop configuration files where useful.

Do not generate implementation code, modify assets, rename files, install dependencies or otherwise change the application during this planning stage.

Once the document is complete, give me a concise summary of:

1. Your assessment of the existing UI.
2. The most important design changes you recommend.
3. The most significant structural changes, if any.
4. Your proposed implementation phases.
5. The decisions you need me to make before proceeding.

**Then stop and wait for my approval.**Project: Omacraft → Asteria
You are working on my Minecraft launcher, currently named Omacraft, which is being rebranded and redesigned as Asteria.
Repository: https://github.com/star-ges/omacraft
The launcher is already essentially feature-complete. Your job is not to reinvent its functionality, but to plan a comprehensive visual and UX redesign that makes it feel like a natural, cohesive part of my personal Linux desktop environment.
Your immediate task is to investigate, evaluate and propose a plan. Do not implement any changes yet.
1. Understand the existing application
First, thoroughly inspect the repository.
Identify the technology stack, architecture, UI framework, styling system and application structure.
Map the existing features, workflows, screens and interactions.
Understand how the frontend communicates with the launcher's underlying functionality.
Identify reusable components and any existing design abstractions.
Find all Omacraft/Omarchy-specific branding, references, assets and assumptions.
Identify technical constraints that could affect a redesign.
Treat the application's existing functionality as established and valuable. Preserve it unless there is a compelling reason to change something, and explain any such proposal.
## 2. Use the StarShell design skill

I have created a Claude Code skill named `starshell-design`, containing the design philosophy developed for my desktop shell.

**Invoke this skill before beginning any design analysis.**

Follow its `SKILL.md` instructions, including reading `design-language.md` in full and inspecting both reference images.

Treat this skill as the authoritative source for Asteria's visual and interaction design.

Do not substitute generic modern UI conventions or your own aesthetic preferences for its principles.

Pay particular attention to:

- Typography as the primary visual element.
- Visible grids and shared 1px hairlines rather than card-based layouts.
- Square corners and the absence of shadows, gradients and blur.
- Neutral resting states, with a single accent reserved for interaction.
- Expressive hover and keyboard-focus behaviour for selectable items.
- Quiet, stable layouts for settings, data and information.
- Motion that responds to interaction rather than running continuously.
- Centralised design tokens and coherent light/dark themes.

The skill's concrete component specifications are defaults, not an inflexible checklist. Apply its underlying principles when designing components or workflows that StarShell itself does not contain.

### Important: Asteria is a first-party redesign

The skill includes guidance about respecting the existing layouts of third-party applications when reskinning them.

That restriction does not apply in the same way here.

Asteria is my own application, and I am explicitly inviting you to reconsider its layout, navigation and information architecture.

I want you to evaluate what the application would look like if it had originally been designed around StarShell's philosophy.

You may propose substantial structural changes where they genuinely improve the experience.

However, distinguish between:

1. Changes that naturally apply the existing StarShell design language.
2. New patterns that extend the design language into application-specific territory.
3. Exceptions that would contradict an established StarShell principle.

For category 3, follow the skill's approval procedure: identify the principle, explain the proposed exception and its benefits, and wait for my approval.

**Do not implement anything during this planning stage.**

### Design exploration

For each major screen, evaluate both:

- **Adaptation:** How could the existing layout be faithfully translated into StarShell's design language?
- **Reimagination:** How would this screen be designed if Asteria had been conceived with StarShell from the beginning?

Recommend whichever approach produces the more coherent and usable result.

Avoid treating every component as a showcase. Follow the skill's distinction between expressive interfaces for choosing and restrained interfaces for reading.

Your eventual design should feel like it belongs alongside StarShell, not like a Minecraft launcher wearing a StarShell costume.2. Use the StarShell design skill
I have created a Claude Code skill named starshell-design, containing the design philosophy developed for my desktop shell.
Invoke this skill before beginning any design analysis.
Follow its SKILL.md instructions, including reading design-language.md in full and inspecting both reference images.
Treat this skill as the authoritative source for Asteria's visual and interaction design.
Do not substitute generic modern UI conventions or your own aesthetic preferences for its principles.
Pay particular attention to:
Typography as the primary visual element.
Visible grids and shared 1px hairlines rather than card-based layouts.
Square corners and the absence of shadows, gradients and blur.
Neutral resting states, with a single accent reserved for interaction.
Expressive hover and keyboard-focus behaviour for selectable items.
Quiet, stable layouts for settings, data and information.
Motion that responds to interaction rather than running continuously.
Centralised design tokens and coherent light/dark themes.
The skill's concrete component specifications are defaults, not an inflexible checklist. Apply its underlying principles when designing components or workflows that StarShell itself does not contain.
Important: Asteria is a first-party redesign
The skill includes guidance about respecting the existing layouts of third-party applications when reskinning them.
That restriction does not apply in the same way here.
Asteria is my own application, and I am explicitly inviting you to reconsider its layout, navigation and information architecture.
I want you to evaluate what the application would look like if it had originally been designed around StarShell's philosophy.
You may propose substantial structural changes where they genuinely improve the experience.
However, distinguish between:
Changes that naturally apply the existing StarShell design language.
New patterns that extend the design language into application-specific territory.
Exceptions that would contradict an established StarShell principle.
For category 3, follow the skill's approval procedure: identify the principle, explain the proposed exception and its benefits, and wait for my approval.
Do not implement anything during this planning stage.
Design exploration
For each major screen, evaluate both:
Adaptation: How could the existing layout be faithfully translated into StarShell's design language?
Reimagination: How would this screen be designed if Asteria had been conceived with StarShell from the beginning?
Recommend whichever approach produces the more coherent and usable result.
Avoid treating every component as a showcase. Follow the skill's distinction between expressive interfaces for choosing and restrained interfaces for reading.
Your eventual design should feel like it belongs alongside StarShell, not like a Minecraft launcher wearing a StarShell costume.
3. Reimagine the launcher as Asteria
Imagine that Asteria was originally conceived and designed specifically for this desktop environment.
Evaluate the existing interface from that perspective.
For every major screen or workflow, consider:
Does its current layout make sense within the desktop's design language?
Is the information hierarchy appropriate?
Could navigation be simplified or made more intuitive?
Are there opportunities to reduce visual clutter?
Would different component structures better express the design philosophy?
Are existing interactions consistent with the rest of the desktop?
Could the application feel more native through better integration with Linux desktop conventions?
Do not assume the existing layout must be preserved.
You have permission to propose substantial changes to screen composition, navigation, component structure and interaction design.
Equally, do not redesign things merely for novelty. If an existing layout is already effective, retain it and explain why.
The goal is a cohesive, polished application, not change for its own sake.
4. Establish Asteria's visual identity
The new name is Asteria.
It is inspired by my online identity, Star, and its association with stars and astronomy.
I want the name to inform the application's identity without turning it into an overtly space-themed or gimmicky interface.
Explore how Asteria can have a recognisable identity while remaining faithful to my desktop's design language.
Consider:
Application icon and branding direction.
Typography and wordmark treatment.
Appropriate use of astronomical motifs, if any.
Colour and accent usage.
Splash screens, loading states and empty states.
Consistent terminology and interface copy.
Prioritise restraint, clarity and consistency. Do not introduce decorative elements that conflict with the desktop's philosophy.
Identify all areas requiring renaming, including user-visible branding, application metadata, packaging, documentation and internal identifiers.
Distinguish cosmetic renaming from changes that could affect compatibility, user data, configuration paths or existing installations.
5. Preserve functionality and reliability
This is a redesign of a working launcher, not a greenfield project.
Maintain feature parity across the application.
Pay particular attention to:
Minecraft installation and launching.
Instance and version management.
Authentication and account handling.
Modding and any supported integrations.
Settings, configuration and persistence.
Downloads, progress reporting and error handling.
Any additional functionality discovered during inspection.
This list is illustrative, not an assumption about which features are implemented.
Avoid unnecessary changes to backend logic or working infrastructure.
If architectural changes would materially improve the UI or maintainability, propose them separately and explain the benefits, risks and migration implications.
6. Produce a design and implementation plan
Before making changes, prepare a detailed proposal for my review.
Structure it as follows:
A. Current-state assessment
Summarise the application's architecture, features, UI structure, strengths and weaknesses.
B. Desktop design language
Summarise the principles extracted from my design brief and desktop implementation. Reference the relevant files.
C. Proposed Asteria design direction
Describe the visual identity, design system, navigation philosophy and overall experience.
D. Screen-by-screen redesign
For each major screen:
Explain what currently exists.
Identify what works and what doesn't.
Describe your proposed design.
Explain why it better fits the desktop.
Classify the change as cosmetic, structural or behavioural.
Where appropriate, compare a conservative restyling approach against a more ambitious redesign.
E. Component and architecture changes
Identify reusable components, design tokens, styling changes and any necessary restructuring.
F. Rebranding and migration
Provide a complete inventory of naming changes, assets, application identifiers and compatibility considerations.
G. Implementation roadmap
Break the work into logical, reviewable phases.
For each phase, specify:
Objectives.
Files or components likely to be affected.
Dependencies.
Risks.
Testing and validation requirements.
Expected visible outcome.
Prioritise changes that establish the design foundation before detailed screen work.
H. Decisions requiring my input
Identify genuine design trade-offs or ambiguities where my preferences would materially affect the result.
Offer reasoned recommendations rather than simply presenting options without guidance.
7. Working principles
Plan first. Do not modify code until I approve the plan.
Read the actual implementation rather than making assumptions.
Treat my desktop design brief as the primary design authority.
Use the existing desktop implementation to validate your interpretation.
Prioritise coherence over visual novelty.
Preserve working functionality.
Avoid unnecessary dependencies and architectural complexity.
Prefer reusable design primitives over one-off styling.
Consider keyboard navigation, accessibility, responsiveness and different window sizes.
Respect the existing project's conventions where they do not conflict with the redesign.
Be willing to challenge existing UI decisions, but justify proposed alternatives.
Distinguish essential changes from optional improvements.
Do not introduce Omarchy branding, dependencies or design assumptions into Asteria.
8. Deliverables
Create a Markdown planning document in the repository, named ASTERIA_REDESIGN_PLAN.md.
The document should be detailed enough to guide a subsequent implementation, but readable enough for me to evaluate the proposed direction without needing to inspect every source file.
Include references to relevant source files and desktop configuration files where useful.
Do not generate implementation code, modify assets, rename files, install dependencies or otherwise change the application during this planning stage.
Once the document is complete, give me a concise summary of:
Your assessment of the existing UI.
The most important design changes you recommend.
The most significant structural changes, if any.
Your proposed implementation phases.
The decisions you need me to make before proceeding.
Then stop and wait for my approval.
