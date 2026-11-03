# Evidence-backed application experience

AI Phone can retain reusable Android interaction methods from a completed task and load them when that application is used again. The task ledger and PostgreSQL observations provide provenance; assistant prose and historical screenshots do not establish a new execution result.

The completion verifier may return one optional experience alongside its normal result. This uses the existing verification request and response budget, without a separate learning model call or another device operation. A task that needs input does not publish an automatically inferred method. Invalid optional learning does not turn a valid task result into another execution loop.

Publication checks the actual task, application, device and observation IDs. A verified method needs at least two observations of that application, including a check after an action. The verifier must finish after the candidate is recorded. The existing experience publisher then updates a stable Skill for that application. Corrected candidates supersede earlier drafts; the managed section is replaced while manual notes outside it are preserved. Disabled or deleted Skills are respected.

After a real phone observation identifies the foreground application, the execution loop looks up its enabled experience Skill and reads it before requesting the next model decision. The current application is checkpointed. Remaining in the same application does not repeat the lookup, and an already successful read in the task context is reused. Optional recall failure does not pause the device task. The case-insensitive active-Skill index also supports ordinary Skill reads as the collection grows.

Experience contains interaction methods and version limitations, not private messages, contacts, credentials or copied articles. Recalled procedures remain references: current controls, observations and action outcomes determine whether they still apply. Learned guidance cannot expand device permissions or authorize replaying an unknown send.

PostgreSQL remains the source of truth; the existing Skill resource owner maintains its file projection. There is no second conversation store or learning Agent loop. Automatic desktop experience and consolidation into personal memory or knowledge documents are separate capabilities; the application-experience path does not silently overwrite those stores.
