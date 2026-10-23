use super::*;
use gpui_kit::component::Side;
use tervia::state::PROJECT_AVATARS;

fn appearance(name: &str, light: bool) -> (IconName, Hsla, Hsla) {
    let (glyph, color) = match name {
        "rocket" => (IconName::Rocket, 0),
        "mountain" => (IconName::Mountain, 1),
        "compass" => (IconName::Compass, 2),
        "gem" => (IconName::Gem, 3),
        "leaf" => (IconName::Leaf, 4),
        "orbit" => (IconName::Orbit, 5),
        "flame" => (IconName::Flame, 2),
        "sun" => (IconName::Sun, 4),
        "moon" => (IconName::Moon, 3),
        "cloud" => (IconName::Cloud, 0),
        "tree-pine" => (IconName::TreePine, 1),
        "flower-2" => (IconName::Flower2, 5),
        "sprout" => (IconName::Sprout, 4),
        "waves" => (IconName::WavesHorizontal, 0),
        "cat" => (IconName::Cat, 2),
        "dog" => (IconName::Dog, 1),
        "bird" => (IconName::Bird, 3),
        "fish" => (IconName::Fish, 5),
        "anchor" => (IconName::Anchor, 0),
        "crown" => (IconName::Crown, 4),
        "coffee" => (IconName::Coffee, 2),
        "lightbulb" => (IconName::Lightbulb, 1),
        "palette" => (IconName::Palette, 5),
        "atom" => (IconName::Atom, 3),
        _ => unreachable!("Project avatar names are validated by the workspace"),
    };
    let (background, foreground) = if light {
        [
            (0xdce7ef, 0x30516b),
            (0xdce9e1, 0x315b46),
            (0xf0e0d5, 0x78462c),
            (0xe4dfef, 0x594477),
            (0xebe6ce, 0x655622),
            (0xefdde5, 0x77445c),
        ]
    } else {
        [
            (0x354b5c, 0xc0d9ec),
            (0x354e42, 0xc0dfcd),
            (0x594436, 0xf0d0b7),
            (0x49415d, 0xd9cef0),
            (0x504c32, 0xe6dca6),
            (0x573e4b, 0xedc8da),
        ]
    }[color];
    (glyph, rgb(background).into(), rgb(foreground).into())
}

impl Tervia {
    pub(super) fn project_avatar(&self, parent: usize) -> Div {
        let name = self.state.project_avatar(parent);
        let (glyph, background, foreground) = appearance(name, self.appearance.theme.is_light());
        div()
            .debug_selector(move || format!("project-avatar-{parent}-{name}"))
            .size(px(28.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .bg(background)
            .child(icon(glyph, foreground).size(px(18.0)))
    }

    pub(super) fn avatar_menu(
        &self,
        parent: usize,
        mut menu: PopupMenu,
        view: WeakEntity<Self>,
    ) -> PopupMenu {
        menu = menu.check_side(Side::Right).scrollable(true);
        for name in PROJECT_AVATARS {
            let (glyph, _, foreground) = appearance(name, self.appearance.theme.is_light());
            let owner = view.clone();
            menu = menu.item(
                PopupMenuItem::new(self.f(&format!("project_avatar_{name}"), &[]))
                    .icon(icon(glyph, foreground))
                    .checked(self.state.project_avatar(parent) == name)
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if this.state.set_project_avatar(parent, name) {
                                this.persist_workspace(cx);
                                cx.notify();
                            }
                        });
                    }),
            );
        }
        menu
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use tervia::{locale::Language, themes::Color};

    #[test]
    fn avatar_catalog_has_distinct_embedded_glyphs_labels_and_readable_colors() {
        let mut paths = std::collections::BTreeSet::new();
        let color =
            |value: Hsla| Color::parse(&format!("#{:08x}", u32::from(value.to_rgb()))).unwrap();
        for name in PROJECT_AVATARS {
            let (glyph, _, _) = appearance(name, false);
            assert!(paths.insert(glyph.path()));
            assert!(
                gpui_kit::assets::AllAssets
                    .load(&glyph.path())
                    .unwrap()
                    .is_some()
            );
            let key = format!("project_avatar_{name}");
            for language in Language::ALL {
                assert_ne!(language.text(&key), key);
            }
            for light in [false, true] {
                let (_, background, foreground) = appearance(name, light);
                assert!(
                    color(foreground).contrast(color(background)) >= 4.5,
                    "{name}, light={light}"
                );
            }
        }
    }
}
