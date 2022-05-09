WITH s
     AS (SELECT barcode,
                newcolor,
                timestamp,
                ROW_NUMBER()
                  OVER (
                    partition BY barcode
                    ORDER BY timestamp DESC ) AS rank
         FROM   event
         WHERE  timestamp <= ?1
    )
SELECT barcode,
       newcolor as color
FROM   s
WHERE  rank = 1;