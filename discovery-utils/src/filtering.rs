/// This defines the types of supported filters
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum FilterType {
    /// If the filter type is Exclude, any items NOT found in the
    /// list are accepted
    Exclude,
    /// If the filter type is Include, only items found in the
    /// list are accepted
    Include,
}

/// The default filter type is `Include`
fn default_action() -> FilterType {
    FilterType::Include
}

/// This defines a filter list.
///
/// The items list can either define the only acceptable
/// items (Include) or can define the only unacceptable items
/// (Exclude)
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FilterList {
    /// This defines a list of items that will be evaluated as part
    /// of the filtering process
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// This defines what the evaluation of items will be.  The default
    /// is `Include`
    #[serde(default = "default_action")]
    pub action: FilterType,
}

/// This tests whether an item should be included according to the `FilterList`
pub fn should_include(filter_list: Option<&FilterList>, item: &str) -> bool {
    if filter_list.is_none() {
        return true;
    }
    let item_contained = filter_list.unwrap().items.contains(&item.to_string());
    if filter_list.as_ref().unwrap().action == FilterType::Include {
        item_contained
    } else {
        !item_contained
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_include() {
        // Test when FilterType::Exclude
        let exclude_items = vec!["beep".to_string(), "bop".to_string()];
        let exclude_filter_list = Some(FilterList {
            items: exclude_items,
            action: FilterType::Exclude,
        });
        assert!(!should_include(exclude_filter_list.as_ref(), "beep"));
        assert!(!should_include(exclude_filter_list.as_ref(), "bop"));
        assert!(should_include(exclude_filter_list.as_ref(), "boop"));

        // Test when FilterType::Exclude and FilterList.items is empty
        let empty_exclude_items = Vec::new();
        let empty_exclude_filter_list = Some(FilterList {
            items: empty_exclude_items,
            action: FilterType::Exclude,
        });
        assert!(should_include(empty_exclude_filter_list.as_ref(), "beep"));

        // Test when FilterType::Include
        let include_items = vec!["beep".to_string(), "bop".to_string()];
        let include_filter_list = Some(FilterList {
            items: include_items,
            action: FilterType::Include,
        });
        assert!(should_include(include_filter_list.as_ref(), "beep"));
        assert!(should_include(include_filter_list.as_ref(), "bop"));
        assert!(!should_include(include_filter_list.as_ref(), "boop"));

        // Test when FilterType::Include and FilterList.items is empty
        let empty_include_items = Vec::new();
        let empty_include_filter_list = Some(FilterList {
            items: empty_include_items,
            action: FilterType::Include,
        });
        assert!(!should_include(empty_include_filter_list.as_ref(), "beep"));

        // Test when None
        assert!(should_include(None, "beep"));
    }
}
