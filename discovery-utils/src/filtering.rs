
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