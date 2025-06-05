class Frequency < ApplicationRecord
  enum :exact_times, { frequency_based: 0, schedule_based: 1 }

  belongs_to :trip
end
