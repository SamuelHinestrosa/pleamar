local titles = { "October invoice", "Shopping list", "pleamar ideas", "Fourth note", "Fifth note" }
local rows = {}
for _, t in titles do table.insert(rows, { title = t }) end
model.rows = rows

on("open", function(i)
    fact.sel = i
    text.status = "Opened: " .. titles[i + 1]
end)

on("created", function()
    text.status = "New note created"
end)
