package shared

import (
	"strings"
)

// RepeatableFlag is an alias to use repeated flags with flag
type RepeatableFlag []string

// String is a method required by flag.Value interface
func (e *RepeatableFlag) String() string {
	result := strings.Join(*e, "\n")
	return result
}

// Set is a method required by flag.Value interface
func (e *RepeatableFlag) Set(value string) error {
	*e = append(*e, value)
	return nil
}
