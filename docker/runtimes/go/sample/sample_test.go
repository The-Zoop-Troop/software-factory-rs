package sample

import "testing"

func TestGreet(t *testing.T) {
	if Greet("rig") != "hello rig" {
		t.Fatal("wrong greeting")
	}
}
