package main

import "fmt"

// Record is one line the CLI writes with --json.
type Record map[string]any

// S is a record's field as text ("" if it has none).
func (r Record) S(k string) string {
	switch v := r[k].(type) {
	case string:
		return v
	case nil:
		return ""
	case float64:
		if v == float64(int64(v)) {
			return fmt.Sprint(int64(v))
		}
		return fmt.Sprint(v)
	default:
		return fmt.Sprint(v)
	}
}
