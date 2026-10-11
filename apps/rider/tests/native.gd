extends SceneTree

var verification: RefCounted

func _initialize() -> void:
    run.call_deferred()

func run() -> void:
    var app = load("res://main.tscn").instantiate()
    root.add_child(app)
    verification = load("res://verification.gd").new()
    verification.run(app)