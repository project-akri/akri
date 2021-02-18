pub mod discovery;
pub mod registration_client;
pub mod filtering;

#[macro_use]
extern crate serde_derive;

#[cfg(test)]
mod tests {
    use super::filtering::{FilterType, FilterList, should_include};

    #[test]
    fn test_should_include() {
        // Test when FilterType::Exclude
        let exclude_items = vec!["beep".to_string(), "bop".to_string()];
        let exclude_filter_list = Some(FilterList {
            items: exclude_items,
            action: FilterType::Exclude,
        });
        assert_eq!(should_include(exclude_filter_list.as_ref(), "beep"), false);
        assert_eq!(should_include(exclude_filter_list.as_ref(), "bop"), false);
        assert_eq!(should_include(exclude_filter_list.as_ref(), "boop"), true);
    
        // Test when FilterType::Exclude and FilterList.items is empty
        let empty_exclude_items = Vec::new();
        let empty_exclude_filter_list = Some(FilterList {
            items: empty_exclude_items,
            action: FilterType::Exclude,
        });
        assert_eq!(
            should_include(empty_exclude_filter_list.as_ref(), "beep"),
            true
        );
    
        // Test when FilterType::Include
        let include_items = vec!["beep".to_string(), "bop".to_string()];
        let include_filter_list = Some(FilterList {
            items: include_items,
            action: FilterType::Include,
        });
        assert_eq!(should_include(include_filter_list.as_ref(), "beep"), true);
        assert_eq!(should_include(include_filter_list.as_ref(), "bop"), true);
        assert_eq!(should_include(include_filter_list.as_ref(), "boop"), false);
    
        // Test when FilterType::Include and FilterList.items is empty
        let empty_include_items = Vec::new();
        let empty_include_filter_list = Some(FilterList {
            items: empty_include_items,
            action: FilterType::Include,
        });
        assert_eq!(
            should_include(empty_include_filter_list.as_ref(), "beep"),
            false
        );
    
        // Test when None
        assert_eq!(should_include(None, "beep"), true);
    }
}
