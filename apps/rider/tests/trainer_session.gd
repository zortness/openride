extends SceneTree
var verification: RefCounted
func _initialize() -> void:
    verification = load("res://trainer_verification.gd").new()
    verification.run.call_deferred(self)
