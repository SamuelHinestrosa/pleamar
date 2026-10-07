every(250, function() fact.ticks += 1 end)

local function locate()
    fact.logic_screens = tonumber(fact["screens.count"]) or 0
    text.logic_monitor = text["screen.0.name"] or ""
    fact.hosted = fact.logic_screens > 0 and text.logic_monitor ~= ""
end
on("fact:screens.count", function()
    fact.screen_events += 1
    locate()
end)
on("text:screen.0.name", locate)
locate()
