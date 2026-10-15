use super::*;

pub(super) fn snapshot(workspace: &Workspace) -> Result<Value, serde_json::Error> {
    let paths = |indices: &[usize]| {
        indices
            .iter()
            .filter(|index| workspace.project_available(**index))
            .map(|index| &workspace.projects[*index].root)
            .collect::<Vec<_>>()
    };
    let mut order = workspace.catalog.order.clone();
    let rows = workspace.all_project_rows();
    for index in &rows {
        if !order.contains(index) {
            order.push(*index);
        }
    }
    Ok(json!({
        "order": serde_json::to_value(paths(&order))?,
        "favorites": serde_json::to_value(paths(&workspace.catalog.favorites.iter().copied().collect::<Vec<_>>()))?,
        "recent": serde_json::to_value(paths(&workspace.catalog.recent))?,
        "closed": serde_json::to_value(paths(&workspace.catalog.closed))?,
        "groups": workspace.catalog.groups.keys().filter(|owner| rows.contains(owner))
            .map(|owner| json!({
                "owner": workspace.projects[*owner].root,
                "target": workspace.projects[workspace.project_target(*owner)].root,
                "members": paths(&workspace.project_members(*owner)),
            })).collect::<Vec<_>>(),
    }))
}

pub(super) fn restore(saved: &Value, workspace: &mut Workspace) -> Result<(), String> {
    let Some(catalog) = saved.get("catalog") else {
        return Ok(());
    };
    if !catalog.is_object() {
        return Err("catalog must be an object".into());
    }
    if let Some(groups) = catalog.get("groups") {
        let groups = groups.as_array().ok_or("catalog groups must be an array")?;
        if groups.len() > 4096 {
            return Err("Too many catalog groups".into());
        }
        let mut total = 0;
        for group in groups {
            let owner = directory(group.get("owner").ok_or("catalog group owner is missing")?)?;
            let target = directory(
                group
                    .get("target")
                    .ok_or("catalog group target is missing")?,
            )?;
            let members = group
                .get("members")
                .and_then(Value::as_array)
                .ok_or("catalog group members must be an array")?;
            total += members.len();
            if total > 4096 {
                return Err("Too many catalog group members".into());
            }
            let mut indices = Vec::new();
            for member in members {
                let path = directory(member)?;
                if path.is_dir() {
                    indices.push(workspace.register_directory(
                        path.canonicalize().map_err(|error| error.to_string())?,
                    ));
                }
            }
            let owner = owner.canonicalize().unwrap_or(owner);
            let target = target.canonicalize().unwrap_or(target);
            if let Some(owner) = workspace
                .projects
                .iter()
                .position(|project| project.root == owner)
            {
                let target = workspace
                    .projects
                    .iter()
                    .position(|project| project.root == target)
                    .unwrap_or(owner);
                if !indices.contains(&owner) {
                    indices.push(owner);
                }
                workspace.catalog.groups.insert(owner, (indices, target));
            }
        }
    }
    let indices = |key| -> Result<Vec<usize>, String> {
        let Some(values) = catalog.get(key) else {
            return Ok(Vec::new());
        };
        let values = values.as_array().ok_or("catalog entries must be arrays")?;
        if values.len() > 4096 {
            return Err("Too many catalog entries".into());
        }
        let mut indices = Vec::new();
        for value in values {
            let root = directory(value)?;
            let root = root.canonicalize().unwrap_or(root);
            let direct = workspace
                .projects
                .iter()
                .position(|project| project.root == root);
            let fallback = || {
                saved
                    .get("projects")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find_map(|project| {
                        let base = directory(project.get("root")?).ok()?;
                        let target = directory(project.get("directory")?).ok()?;
                        if root != base && root != target {
                            return None;
                        }
                        workspace
                            .projects
                            .iter()
                            .position(|project| project.root == base || project.root == target)
                    })
            };
            if let Some(index) = direct.or_else(fallback)
                && !indices.contains(&index)
            {
                indices.push(index);
            }
        }
        Ok(indices)
    };
    let order = indices("order")?;
    let favorites = indices("favorites")?;
    let recent = indices("recent")?;
    let closed = indices("closed")?;
    workspace.catalog.order = order;
    workspace.catalog.favorites = favorites.into_iter().collect();
    // Closing updates active selection; restore the captured visit order only after applying it.
    let active = workspace.active_project;
    for owner in closed.into_iter().rev() {
        workspace.close_project(owner);
    }
    if workspace.project_visible(active) {
        workspace.switch_project(active);
    }
    workspace.catalog.recent = recent;
    Ok(())
}
