# Respond to PR Review Comments

Fetch all open review comments on the current branch's pull request, then address each one — with code changes where warranted and a reply in every case.

## Steps

1. **Identify the PR**: Run `gh pr view --json number,headRefName` to get the PR number for the current branch. If none exists, tell the user and stop.

2. **Fetch inline review comments**: Run:
   ```
   gh api repos/{owner}/{repo}/pulls/{pr_number}/comments
   ```
   Parse the JSON. For each comment, extract: `id`, `path`, `line`, `body`, `user.login`, and `in_reply_to_id`. Skip comments that have `in_reply_to_id` set — only process top-level thread starters.

3. **Categorize each comment** as one of:
   - **Code fix required** — points out a real bug, incorrect behavior, missing update, or necessary improvement (not purely style preference)
   - **Reply only** — raises a concern that is intentional, already correct, or where the tradeoff should be explained rather than changed

4. **Present a plan** — list every comment with its category and intended action before making any changes.

5. **Apply code fixes** (for "Code fix required" comments):
   - Read the relevant file(s) first before editing
   - Make the minimal targeted fix; do not refactor surrounding code
   - After all fixes are applied, run:
     ```
     cargo fmt --all -- --check
     cargo clippy --all-targets --all-features -- -D warnings
     cargo test
     ```
   - Fix any failures before proceeding

6. **Commit and push** (only if code changes were made):
   - Stage only the modified files
   - Commit message: single line, under 72 chars, `type: description` format
   - Push to the current branch

7. **Reply to every comment**:
   ```
   gh api repos/{owner}/{repo}/pulls/comments/{comment_id}/replies \
     --method POST \
     --field body="..."
   ```
   - **Code fix** replies: mention the commit SHA and briefly describe what changed
   - **Reply only** replies: explain the reasoning — why the current approach is correct or what tradeoff was made

8. **Summarize** — report which comments got code fixes (with commit SHA) and which got explanatory replies only.
