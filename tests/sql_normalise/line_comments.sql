SELECT id, -- primary key
       name, -- display name
       email
FROM users -- the main table
-- filter to active records only
WHERE active = 1
