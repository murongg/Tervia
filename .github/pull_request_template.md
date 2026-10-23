<!-- English and Chinese are welcome / 欢迎使用中文或英文填写。 -->
<!-- Keep the PR focused. Remove optional sections that do not apply. -->

## Summary

<!-- Describe the problem, what changed, and the resulting behavior. -->

## Related issues

<!-- Link related issues. Use "Closes #123" only when this PR fully resolves one. -->

## Validation

<!-- List exact commands and results, plus manual checks and platforms tested.
For code changes, use the applicable CI checks:
- cargo fmt --all -- --check
- cargo test --locked --features ui-tests
- cargo clippy --locked --features ui-tests --all-targets -- -D warnings
For Windows PTY changes: cargo test --locked --features pty
For release / packaging scripts: python3 -B -m unittest discover -s tests -p 'test_*.py'
For docs-only changes, check links and formatting; explain any checks not run.
-->

## Screenshots or recordings

<!-- For UI changes, attach useful visual evidence here when needed.
Keep temporary captures outside the repository; commit images only when explicitly
requested or needed as referenced documentation illustrations. -->

## Risks and compatibility

<!-- Note affected platforms, config or persisted-data changes, breaking changes,
and known limitations. Include migration or rollback steps when needed. -->

## Checklist

- [ ] I verified the change and reported the results above, including checks not run.
- [ ] Behavior changes have regression coverage where applicable; test data is synthetic.
- [ ] Relevant documentation and translations are updated, or no updates are needed.
