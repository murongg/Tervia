use std::{collections::BTreeMap, path::PathBuf};

use super::tree::{Entry, EntryKind};

// Common file families, inspired by https://github.com/antfu/vscode-file-nesting-config.
// Specific anchors precede broad rules so an environment file keeps its own variants.
const RULES: &[(&str, &str)] = &[
    (".env", "*.env, .env.*, .envrc, env.d.ts"),
    (
        "Dockerfile",
        "*.dockerfile, .dockerignore, .devcontainer.*, Dockerfile*, dockerfile*, compose.*, docker-compose.*",
    ),
    (
        ".gitignore",
        ".gitattributes, .gitmodules, .gitmessage, .git-blame*, .lfsconfig, .mailmap",
    ),
    (
        "Cargo.toml",
        "Cargo.lock, .clippy.toml, clippy.toml, .rustfmt.toml, rustfmt.toml, rust-toolchain, rust-toolchain.toml, cross.toml",
    ),
    (
        "pyproject.toml",
        "poetry.lock, poetry.toml, uv.lock, uv.toml, pdm.lock, .pdm.toml, .python-version, .flake8, .isort.cfg, requirements*.txt, requirements*.in, setup.cfg, setup.py, tox.ini, ruff.toml, .ruff.toml",
    ),
    (
        "Pipfile",
        "Pipfile.lock, .python-version, requirements*.txt",
    ),
    ("go.mod", "go.sum, .air.toml"),
    ("go.work", "go.work.sum"),
    ("composer.json", "composer.lock, phpunit.xml*, psalm*.xml"),
    ("Gemfile", "Gemfile.lock, .ruby-version"),
    (
        "build.gradle.kts",
        "settings.gradle.kts, gradlew, gradlew.bat, gradle.properties, gradle.lockfile",
    ),
    (
        "build.gradle",
        "settings.gradle, gradlew, gradlew.bat, gradle.properties, gradle.lockfile",
    ),
    (
        "pubspec.yaml",
        "pubspec.lock, pubspec_overrides.yaml, analysis_options.yaml, build.yaml, .metadata, .packages",
    ),
    (
        "package.json",
        "package-lock.json, npm-shrinkwrap.json, pnpm*, yarn*, bun.lock, bun.lockb, bunfig.toml, .npm*, .pnpm*, .yarnrc*, .pnp.*, .nvmrc, .node-version, .browserslist*, .editorconfig, .eslint*, eslint.config.*, .prettier*, prettier.config.*, biome.json*, .swcrc, .babelrc*, babel.config.*, .stylelint*, stylelint.config.*, .lintstagedrc*, lint-staged.config.*, .commitlint*, commitlint.config.*, .releaserc*, release.config.*, lerna.json, nx.json, turbo.json, .npmignore",
    ),
    ("tsconfig.json", "tsconfig.*.json, tsconfig.tsbuildinfo"),
    ("vite.config.*", "vitest.config.*, vite-env.d.ts"),
    ("next.config.*", "next-env.d.ts, next-i18next.config.*"),
    ("README.md", "README.*.md"),
    (
        "*.ts",
        "$(capture).js, $(capture).d.ts, $(capture).ts.map, $(capture).test.ts, $(capture).spec.ts",
    ),
    (
        "*.tsx",
        "$(capture).js, $(capture).jsx, $(capture).d.ts, $(capture).test.tsx, $(capture).spec.tsx, $(capture).stories.tsx, $(capture).module.css, $(capture).module.scss",
    ),
    (
        "*.jsx",
        "$(capture).js, $(capture).test.jsx, $(capture).spec.jsx, $(capture).stories.jsx, $(capture).module.css, $(capture).module.scss",
    ),
    (
        "*.js",
        "$(capture).js.map, $(capture).min.js, $(capture).test.js, $(capture).spec.js",
    ),
    (
        "*.vue",
        "$(capture).test.ts, $(capture).spec.ts, $(capture).stories.ts",
    ),
    (
        "*.svelte",
        "$(capture).test.ts, $(capture).spec.ts, $(capture).stories.ts",
    ),
    ("*.css", "$(capture).css.map, $(capture).min.css"),
    ("*.scss", "$(capture).css, $(capture).scss.map"),
    ("*.go", "$(capture)_test.go"),
];

#[derive(Default)]
pub(super) struct Nesting {
    pub roots: Vec<usize>,
    pub children: BTreeMap<PathBuf, Vec<usize>>,
}

impl Nesting {
    pub fn new(entries: &[Entry]) -> Self {
        let files: BTreeMap<_, _> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.kind == EntryKind::File)
            .filter_map(|(index, entry)| Some((entry.path.file_name()?.to_str()?, index)))
            .collect();
        let mut parents = vec![None; entries.len()];
        for (pattern, children) in RULES {
            for (&name, &parent) in &files {
                let Some(captured) = capture(pattern, name) else {
                    continue;
                };
                for child in children.split(", ") {
                    let child = child.replace("$(capture)", captured);
                    if child.contains('*') {
                        for (&name, &index) in &files {
                            if capture(&child, name).is_some() {
                                assign(&mut parents, parent, index);
                            }
                        }
                    } else if let Some(&index) = files.get(child.as_str()) {
                        assign(&mut parents, parent, index);
                    }
                }
            }
        }
        let mut nesting = Self::default();
        // Flatten transitive matches into one group; each real file appears exactly once.
        for index in 0..entries.len() {
            let mut root = index;
            while let Some(parent) = parents[root] {
                root = parent;
            }
            if root == index {
                nesting.roots.push(index);
            } else {
                nesting
                    .children
                    .entry(entries[root].path.clone())
                    .or_default()
                    .push(index);
            }
        }
        nesting
    }
}

fn capture<'a>(pattern: &str, name: &'a str) -> Option<&'a str> {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        name.strip_prefix(prefix)?.strip_suffix(suffix)
    } else {
        (pattern == name).then_some("")
    }
}

fn assign(parents: &mut [Option<usize>], parent: usize, child: usize) {
    if parent == child || parents[child].is_some() {
        return;
    }
    // Overlapping rules must never form a cycle that makes an entire family unreachable.
    let mut ancestor = parent;
    loop {
        if ancestor == child {
            return;
        }
        match parents[ancestor] {
            Some(parent) => ancestor = parent,
            None => break,
        }
    }
    parents[child] = Some(parent);
}
